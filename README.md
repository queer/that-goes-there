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
