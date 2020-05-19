## darwinia-sync

### Setup

1.
	```sh
	cargo install darwinia-sync
	```
2.	```sh
	git clone https://github.com/AurevoirXavier/darwinia-sync.git
	cd darwinia-sync
	cargo build --release
	cp target/release/darwinia-sync ~/.local/bin # or somewhere in your $PATH
	```

### Usage

#### Running Example
```sh
# normal
darwinia-sync -l -s /home/xavier/crab/crab.sh

# pm2
pm2 start darwinia-sync -- -l -s /home/xavier

# systemd
systemctl start crab.service
```

#### `crab.sh` Script Example
```sh
/home/xavier/crab/darwinia \
	--unsafe-rpc-external \
	--unsafe-ws-external \
	--validator \
	--base-path /home/xavier/crab/data/tester \
	--name Xavier \
	--rpc-cors all
```

#### `crab.service` Systemd Example
```service
[Unit]
Description=Crab

[Service]
ExecStart=/home/xavier/.cargo/bin/darwinia-sync -l -s /home/xavier/crab/crab.sh

[Install]
WantedBy=multi-user.target
```

#### Help
```sh
λ darwinia-sync --help
darwinia-sync 0.7.0
Xavier Lau <c.estlavie@icloud>
Darwinia Maintain Tool

USAGE:
    darwinia-sync [FLAGS] [OPTIONS]

FLAGS:
    -l, --log        Syncing Log
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -s, --script <PATH>    Darwinia Boot Script
```
