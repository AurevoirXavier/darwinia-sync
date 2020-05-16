mod pattern {
	#![allow(non_upper_case_globals)]

	// --- crates ---
	use lazy_static::lazy_static;
	use regex::Regex;

	lazy_static! {
		pub static ref best_number: Regex = Regex::new(r".+?best.+?#(\d+)").unwrap();
	}
}

// --- std ---
use std::{
	env,
	io::{BufRead, BufReader},
	process::{Command, Stdio},
	sync::{
		atomic::{AtomicBool, Ordering},
		mpsc, Arc,
	},
	thread,
	time::Duration,
};
// --- crates ---
use sysinfo::{ProcessExt, System, SystemExt};

type PID = i32;

fn main() {
	pretty_env_logger::init_custom_env("SYNC_LOG");

	let running = Arc::new(AtomicBool::new(true));
	{
		let r = running.clone();
		ctrlc::set_handler(move || {
			r.store(false, Ordering::SeqCst);
		})
		.unwrap();
	}
	while running.load(Ordering::SeqCst) {
		let pid = run();
		kill(pid);
	}
}

fn run() -> PID {
	let (tx, rx) = mpsc::channel();
	let mut darwinia = Command::new(
		env::args()
			.skip(1)
			.next()
			.expect("usage: darwinia-sync ./your_boot_script"),
	)
	.stdout(Stdio::null())
	.stderr(Stdio::piped())
	.spawn()
	.unwrap();
	let stderr = BufReader::new(darwinia.stderr.take().unwrap());
	let darwinia_thread = thread::spawn(move || {
		let (mut best_number, mut idel_times) = (0, 0);
		for log in stderr.lines() {
			let log = &log.unwrap();
			sync_stalled(log, &mut best_number, &mut idel_times);
			if idel_times > 3 {
				tx.send(255u8).unwrap();
				break;
			} else {
				println!("{}", log);
			}
		}
	});

	while let Ok(received) = rx.recv() {
		match received {
			255 => {
				darwinia_thread.join().unwrap();
				darwinia.kill().unwrap();
				kill(darwinia.id() as _);
				thread::sleep(Duration::from_secs(3));

				break;
			}
			_ => (),
		}
	}

	darwinia.id() as _
}

fn sync_stalled(log: &str, previous_best_number: &mut u32, idel_times: &mut u8) {
	if let Some(captures) = pattern::best_number.captures(log) {
		if let Some(best_number) = captures.get(1) {
			let best_number = best_number.as_str().parse().unwrap();
			if *previous_best_number == best_number {
				*idel_times += 1;
			} else {
				*previous_best_number = best_number;
				*idel_times = 0;
			}

			log::trace!("Best Number: {}, Idle Times: {}", best_number, idel_times);
		}
	}
}

fn kill(darwinia_pid: PID) {
	let sys = System::new_all();
	for offset in 0..=2 {
		let darwinia_pid = darwinia_pid + offset;
		if let Some(process) = sys.get_process(darwinia_pid) {
			if process.name().contains("darwinia") {
				Command::new("kill")
					.args(&["-9", &darwinia_pid.to_string()])
					.output()
					.unwrap();
			}
		}
	}
}
