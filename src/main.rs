mod global {
	#![allow(non_upper_case_globals)]

	// --- crates ---
	use lazy_static::lazy_static;
	use regex::Regex;
	// --- custom ---
	use crate::*;

	lazy_static! {
		pub static ref darwinia_sync_pid: pid_t = process::id() as _;
		pub static ref best_number_regex: Regex = Regex::new(r".+?best.+?#(\d+)").unwrap();
	}
}

// --- std ---
use std::{
	env,
	io::{BufRead, BufReader},
	process::{self, Command, Stdio},
	time::Duration,
};
// --- crates ---
use clap::{app_from_crate, Arg};
use libc::pid_t;
#[cfg(target_os = "linux")]
use procfs::process::Process;
use sysinfo::{ProcessExt, System, SystemExt};

type Status = u8;

const PID_OFFSET: pid_t = 10;

const UNKNOWN: Status = 0;
const CRASHED: Status = 1;
const DB_LOCKED: Status = 254;
const IDLED: Status = 255;

const IDLED_LIMIT: u8 = 3;

fn main() {
	let matches = app_from_crate!()
		.arg(Arg::new("log").short('l').long("log").about("Syncing Log"))
		.arg(
			Arg::new("script")
				.short('s')
				.long("script")
				.value_name("PATH")
				.about("Darwinia Boot Script")
				.takes_value(true),
		)
		.get_matches();

	if matches.is_present("log") {
		env::set_var("SYNC_LOG", "trace");
		pretty_env_logger::init_custom_env("SYNC_LOG");
	}

	if let Some(script_path) = matches.value_of("script") {
		for restart_times in 0.. {
			match run(&script_path, restart_times) {
				CRASHED => {
					sleep("Crashed", 5);
				}
				DB_LOCKED => {
					sleep("DB Locked", 5);
				}
				IDLED => {
					sleep("Idled", 3);
				}
				_ => break,
			}
		}
	}
}

fn run(script_path: &str, restart_times: u32) -> Status {
	let mut darwinia = Command::new(script_path)
		.stdout(Stdio::null())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	let darwinia_tid = darwinia.id() as pid_t;
	let stderr = BufReader::new(darwinia.stderr.take().unwrap());
	let mut status = UNKNOWN;
	let (mut best_number, mut idle_times) = (0, 0);

	for log in stderr.lines() {
		let log = &log.unwrap();
		sync_stalled(
			log,
			&mut best_number,
			&mut idle_times,
			|best_number, idle_times| {
				#[cfg(target_os = "linux")]
				if let Ok(darwinia_sync_process) = Process::myself() {
					log::trace!("Darwinia-Sync vsize: {}", darwinia_sync_process.stat.vsize,);
				}
				#[cfg(target_os = "linux")]
				if let Ok(darwinia_process) = Process::new(darwinia_tid) {
					log::trace!("Darwinia vsize: {}", darwinia_process.stat.vsize,);
				}

				log::trace!(
					"Darwinia-Sync PID: {}, Darwinia-Sync TID: {}, Restart Times: {}",
					*global::darwinia_sync_pid,
					darwinia_tid,
					restart_times,
				);
				log::trace!("Best Number: {}, Idle Times: {}", best_number, idle_times);
			},
		);

		if idle_times > IDLED_LIMIT {
			status = IDLED;
			break;
		} else if db_locked(log) {
			status = DB_LOCKED;
			break;
		} else {
			println!("{}", log);
		}
	}

	match status {
		CRASHED | DB_LOCKED | IDLED => {
			for pid_offset in 1..=PID_OFFSET {
				let darwinia_pid = darwinia_tid + pid_offset;
				let s = System::new();
				if let Some(process) = s.get_process(darwinia_pid) {
					if process.name().contains("darwinia") {
						kill(darwinia_pid);
					}
				}
			}
			kill(darwinia_tid);
			let _ = darwinia.kill();
			let _ = darwinia.wait();
		}
		_ => (),
	}

	status
}

fn db_locked(log: &str) -> bool {
	log.contains("db/LOCK")
}

fn sync_stalled<F>(log: &str, previous_best_number: &mut u32, idle_times: &mut u8, logger: F)
where
	F: FnOnce(u32, u8),
{
	if let Some(captures) = global::best_number_regex.captures(log) {
		if let Some(best_number) = captures.get(1) {
			let best_number = best_number.as_str().parse().unwrap();
			if *previous_best_number == best_number {
				*idle_times += 1;
			} else {
				*previous_best_number = best_number;
				*idle_times = 0;
			}
		}

		logger(*previous_best_number, *idle_times);
	}
}

fn kill(pid: pid_t) {
	unsafe {
		libc::killpg(pid, 9);
		libc::kill(pid, 9);
	}
}

fn sleep(log: &str, secs: u64) {
	for i in (0..secs).rev() {
		log::trace!("{}", format!("{}, Restarting in {}", log, i));
		std::thread::sleep(Duration::from_secs(1));
	}
}
