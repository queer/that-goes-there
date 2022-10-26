# that goes there

(hopefully) fast configuration management.

rust: stable (1.64 min tested version)

## setup

install [pre-commit](https://pre-commit.com/).

```bash
pre-commit install
pre-commit autoupdate
```

## but we already have ansible/salt/chef/puppet/...!

needed a config management tool that could use ssh and agents as transports
dynamically.

## todo

- pretty tui with [cursive](https://crates.io/crates/cursive) for log streaming
- send command logs to their own sink for tui display eventually
- server+agent executor
