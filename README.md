# that goes there

*that goes there* (hereafter "there" or `there`) is:

- A library for general planning and execution of tasks on local and remote
  hosts.
  - [crates.io](https://crates.io/crates/there)
  - Tasks are compiled down to `sh(1)`-compatible commands for execution.
  - Non-raw-command tasks attempt to validate prerequists before execution.
- A CLI for applying plans via:
  - Local execution
  - Remote SSH
  - Agent-controller SSH
- An agent-controller pair for remote SSH execution.

Minimum supported Rust version: `1.66`

## features

### Library:

- Tokio-based, fully-async
- Pluggable execution backends
  - [x] Local
  - [x] SSH
  - [ ] Implement `Executor` yourself!
- Async log streaming
- Pluggable log streaming backends
  - [x] In-memory
  - [ ] Implement `LogSource`/`LogSink` yourself!
- Agent-controller shared IPC structs
- Plan validation
  - Compile tasks down to `sh(1)`-compatible commands
- Host files + host groups
- Fully instrumented with `tracing::instrument`
  - `tracing`-based logging that can be consumed with `tracing-subscriber`
  - `color_eyre`-based error reporting
    - Used to share error implementation between CLI/agent/controller

### CLI:

- Log streaming by default
  - [x] Local execution
  - [x] SSH execution
  - [x] Agent-controller execution
- Plan validation

## setup

Install [pre-commit](https://pre-commit.com/).

```bash
pre-commit install
pre-commit autoupdate
cargo install cargo-audit
```

## usage

kinda undocumented for now. refer to `test/*.yaml` for some plan/hostfile
examples.

## todo

- Pretty tui with [makeup](https://crates.io/crates/makeup) for log streaming

## show me something pretty

```
git:(mistress) | ▶  cargo run -p there-cli -- plan apply --dry -f ./test/ssh-plan.yaml --hosts ./test/ssh-hosts.yaml # ...
*** plan: test plan ***

* metadata
** hosts:
*** group: ssh-group
**** broken-ssh: localhost:2222 (ssh)
**** ssh-localhost: localhost:22 (ssh)
** test command: echo hello world!
*** ExeExists { exe: "echo" }
** test command 2: echo hello world!! :D
*** ExeExists { exe: "echo" }
** test command 2: echo wow!!!!!
*** ExeExists { exe: "echo" }
** create some file: touch /tmp/some-file
*** ExeExists { exe: "touch" }
*** DirectoryExists { path: "/tmp" }
git:(mistress) 1 | ▶  cargo run -p there-cli -- plan apply -f ./test/ssh-plan.yaml --hosts ./test/ssh-hosts.yaml # ...
*** applying plan to group: ssh-group ***
*** prepared plan for host: broken-ssh
*** prepared plan for host: ssh-localhost
broken-ssh: * steps: 4
ssh-localhost: * steps: 4
broken-ssh: ssh authentication failed!
*** failed plan: test plan for host: broken-ssh: 0/4 ***
*** error: ssh executor failed to apply plan test plan to host broken-ssh: 0/4 tasks finished: ssh authentication failed!
ssh-localhost: ** executing task: test command
ssh-localhost: ensuring ExeExists { exe: "echo" }
ssh-localhost: /bin/echo
ssh-localhost:
ssh-localhost: hello world!
ssh-localhost:
ssh-localhost:
ssh-localhost: ** executing task: test command 2
ssh-localhost: ensuring ExeExists { exe: "echo" }
ssh-localhost: /bin/echo
ssh-localhost:
ssh-localhost: hello world!! :D
ssh-localhost:
ssh-localhost:
ssh-localhost: ** executing task: test command 2
ssh-localhost: ensuring ExeExists { exe: "echo" }
ssh-localhost: /bin/echo
ssh-localhost:
ssh-localhost: wow!!!!!
ssh-localhost:
ssh-localhost:
ssh-localhost: ** executing task: create some file
ssh-localhost: ensuring ExeExists { exe: "touch" }
ssh-localhost: /bin/touch
ssh-localhost:
ssh-localhost: ensuring DirectoryExists { path: "/tmp" }
ssh-localhost:
ssh-localhost: *** finished applying plan: test plan -> ssh-localhost (4/4)
*** completed plan: test plan for host: ssh-localhost: 4/4 ***
git:(mistress) 1 | ▶
```
