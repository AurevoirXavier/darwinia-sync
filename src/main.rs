mod global {
	#![allow(non_upper_case_globals)]

	// --- crates ---
	use lazy_static::lazy_static;
	use regex::Regex;

	lazy_static! {
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
use sysinfo::{ProcessExt, RefreshKind, Signal, System, SystemExt};

type Status = u8;

const UNKNOWN: Status = 0;
const CRASHED: Status = 1;
const DB_LOCKED: Status = 254;
const IDLED: Status = 255;

const IDLED_LIMIT: u8 = 5;

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
	let sync_pid = process::id() as pid_t;

	let mut system = System::new_with_specifics(
		RefreshKind::new()
			.with_processes()
			.with_cpu()
			.with_memory()
			.with_disks()
			.with_networks(),
	);

	let mut sync_thread = Command::new(script_path)
		.stdout(Stdio::null())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();

	let stderr = BufReader::new(sync_thread.stderr.take().unwrap());
	let mut status = UNKNOWN;
	let (mut best_number, mut idle_times) = (0, 0);

	for log in stderr.lines() {
		refresh_system(&mut system);

		let sync_thread_detail = system.get_process(sync_pid).unwrap();
		let log = &log.unwrap();

		check_sync(
			log,
			&mut best_number,
			&mut idle_times,
			|best_number, idle_times| {
				let cpu_usage = sync_thread_detail.cpu_usage();
				let memory_usage = sync_thread_detail.memory();
				let disk_usage = sync_thread_detail.disk_usage();
				log::trace!("Sync PID: {}, Restart Times: {}", sync_pid, restart_times);
				log::trace!(
					"Cpu Usage: {}%, Memory Usage: {} KB",
					cpu_usage,
					memory_usage,
				);
				log::trace!("Disk Usage:");
				log::trace!(
					"\t[read bytes   : new/total => {}/{}]",
					disk_usage.read_bytes,
					disk_usage.total_read_bytes,
				);
				log::trace!(
					"\t[written bytes: new/total => {}/{}]",
					disk_usage.written_bytes,
					disk_usage.total_written_bytes,
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
			killpg_except_root(&mut system);
			let _ = sync_thread.kill();
			let _ = sync_thread.wait();
		}
		_ => (),
	}

	status
}

fn db_locked(log: &str) -> bool {
	log.contains("db/LOCK")
}

fn check_sync<F>(log: &str, previous_best_number: &mut u32, idle_times: &mut u8, logger: F)
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

fn sleep(log: &str, secs: u64) {
	for i in (1..=secs).rev() {
		log::trace!("{}", format!("{}, Restarting in {}", log, i));
		std::thread::sleep(Duration::from_secs(1));
	}
}

fn refresh_system(system: &mut System) {
	system.refresh_processes();
	system.refresh_system();
	system.refresh_disks();
	system.refresh_networks();
}

fn killpg_except_root(system: &mut System) {
	refresh_system(system);

	let sync_pid = process::id() as pid_t;
	let gid = unsafe { libc::getpgrp() };

	for (&pid, process) in system.get_processes() {
		if pid > sync_pid && unsafe { libc::getpgid(pid) } == gid {
			process.kill(Signal::Kill);
		}
	}
}
