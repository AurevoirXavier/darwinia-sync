mod global {
	#![allow(non_upper_case_globals)]

	// --- crates ---
	use lazy_static::lazy_static;
	use regex::Regex;
	// --- custom ---
	use crate::*;

	lazy_static! {
		pub static ref darwinia_sync_pid: PID = process::id() as _;
		pub static ref best_number_regex: Regex = Regex::new(r".+?best.+?#(\d+)").unwrap();
	}
}

// --- std ---
use std::{
	cell::RefCell,
	env,
	io::{BufRead, BufReader},
	process::{self, Command, Stdio},
	sync::{
		atomic::{AtomicBool, Ordering},
		mpsc, Arc,
	},
	thread,
	time::Duration,
};
// --- crates ---
use clap::{app_from_crate, Arg};
#[cfg(target_os = "linux")]
use procfs::process::Process;

type PID = i32;

const INIT: u8 = 0;
const EXITED: u8 = 255;

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
		let running = Arc::new(AtomicBool::new(true));
		{
			let r = running.clone();
			ctrlc::set_handler(move || {
				r.store(false, Ordering::SeqCst);
			})
			.unwrap();
		}
		let status = RefCell::new(INIT);

		while running.load(Ordering::SeqCst) {
			run(script_path, status.clone());

			// TODO
			match status.replace(INIT) {
				EXITED => (),
				_ => (),
			}

			thread::sleep(Duration::from_secs(3));
		}

		kill(*global::darwinia_sync_pid);
	}
}

fn run(script_path: &str, status: RefCell<u8>) {
	let (tx, rx) = mpsc::channel();
	let mut darwinia = Command::new(script_path)
		.stdout(Stdio::null())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	let stderr = BufReader::new(darwinia.stderr.take().unwrap());
	let darwinia_pid = darwinia.id() as PID;
	let darwinia_thread = thread::spawn(move || {
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
						log::trace!(
							"Darwinia-Sync utime: {}, stime: {}, cutime: {}, cstime: {}, vsize: {}",
							darwinia_sync_process.stat.utime,
							darwinia_sync_process.stat.stime,
							darwinia_sync_process.stat.cutime,
							darwinia_sync_process.stat.cstime,
							darwinia_sync_process.stat.vsize,
						);
					}
					#[cfg(target_os = "linux")]
					if let Ok(darwinia_process) = Process::new(darwinia_pid) {
						log::trace!(
							"Darwinia utime: {}, stime: {}, cutime: {}, cstime: {}, vsize: {}",
							darwinia_process.stat.utime,
							darwinia_process.stat.stime,
							darwinia_process.stat.cutime,
							darwinia_process.stat.cstime,
							darwinia_process.stat.vsize,
						);
					}

					log::trace!(
						"Darwinia-Sync PID: {}, Darwinia-Sync TID: {}, Darwinia PID: {}",
						*global::darwinia_sync_pid,
						darwinia_pid,
						darwinia_pid + 1,
					);
					log::trace!("Best Number: {}, Idle Times: {}", best_number, idle_times);
				},
			);

			if idle_times > 3 {
				tx.send(255u8).unwrap();
				break;
			} else {
				println!("{}", log);
			}
		}
	});

	while let Ok(received) = rx.recv() {
		*status.borrow_mut() = received;
		match received {
			EXITED => {
				darwinia_thread.join().unwrap();
				darwinia.kill().unwrap();
				kill(darwinia_pid);
				kill(darwinia_pid + 1);

				break;
			}
			_ => (),
		}
	}
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

fn kill(pid: PID) {
	Command::new("kill")
		.args(&["-9", &pid.to_string()])
		.output()
		.unwrap();
}
