# that goes there

(hopefully) fast configuration management.

rust: stable (1.64 min tested version)

## setup

install [pre-commit](https://pre-commit.com/).

```bash
pre-commit install
pre-commit autoupdate
yarn global add markdownlint-cli # or npm i -g markdownlint-cli
```

## but we already have ansible/salt/chef/puppet/etc

needed a config management tool that could use ssh and agents as transports
dynamically. also wanted to experiment with some ui/ux stuff. also it's fun!

## todo

- pretty tui with [cursive](https://crates.io/crates/cursive) for log streaming
- server+agent executor
  - rkyv?
  - grpc streams? (ew)
  - websockets???
  - http requests?????
