---
name: examples-checker
description: Check Ankaios examples by building and running each example in the devcontainer and validating they work correctly. Use this when asked to "check examples", "test examples", or "verify examples".
---

# Examples Checker Skill

Use this skill to validate Ankaios examples by building, running, and verifying their expected behavior inside the devcontainer.

## Trigger behavior

- If the user asks to "check examples" (or equivalent), run all examples by default.
- If the user specifies a particular example name (e.g. `rust_sdk_hello`), only check that one.
- If the user asks to "check control interface examples", run only the four control interface examples.
- If the user asks to "check SDK examples", run only the SDK examples.

## Example discovery

- Discover example subfolders at runtime by listing `/workspaces/ankaios/examples/`.
- Exclude `tools/` folder.

## Example categories

### Control Interface examples (raw protobuf)

| Example                    | Language |
| -------------------------- | -------- |
| `rust_control_interface`   | Rust     |
| `cpp_control_interface`    | C++      |
| `nodejs_control_interface` | Node.js  |
| `python_control_interface` | Python   |

**Expected behavior:** Each creates a `dynamic_nginx` workload via the Control Interface, then polls workload states every 5 seconds. The example workload itself should reach `Running(Ok)`.

### SDK examples

| Example                  | SDK    | Description                                            |
| ------------------------ | ------ | ------------------------------------------------------ |
| `python_sdk_hello`       | Python | Creates `dynamic_nginx`, waits for RUNNING, deletes it |
| `python_sdk_interactive` | Python | Starts a sleeping container for manual SDK exploration |
| `python_sdk_logging`     | Python | Reads logs from a `screamer` workload                  |
| `rust_sdk_hello`         | Rust   | Creates `dynamic_nginx`, waits for RUNNING, deletes it |
| `rust_sdk_logging`       | Rust   | Reads logs from a `screamer` workload                  |

## Prerequisites check

Before running any example, verify:

1. Ankaios binaries are built and available. Check for debug binaries first:
   - Debug: `/workspaces/ankaios/target/x86_64-unknown-linux-musl/debug/`
   - Release: `/workspaces/ankaios/target/x86_64-unknown-linux-musl/release/`
   - If neither exists, build with `cargo build`.
2. No stale Ankaios processes are running: run the cleanup script before starting.
3. Set `ANK_BIN_DIR` to the directory containing the Ankaios binaries (debug or release).

## Existing scripts

The `examples/` folder provides two helper scripts that **must** be used:

- **`run_example.sh <example_name>`** — Builds the container image, starts Ankaios (server + agent_A) if not already running, and applies the example manifest. Accepts `--ankaios-bin-dir` or uses the `ANK_BIN_DIR` environment variable.
- **`cleanup.sh`** — Stops Ankaios server and agent, stops and removes all podman containers and volumes.

## Execution process

### Per-example execution

For each example to check:

1. **Clean state** — Run `examples/cleanup.sh` to ensure no leftover state.
2. **Run the example** — From the `examples/` directory, run in the foreground and wait for it to finish:

   ```shell
   ./run_example.sh <example_name>
   ```

   This script handles: building the container image, starting Ankaios (server + agent) in the background, and applying the manifest. The script returns once the build is done and Ankaios is started.
   If the build fails, record the error and move to the next example.
3. **Wait for workloads** — Poll with `ank get workloads` every 5 seconds, up to 60 seconds, until the example workload reaches `Running(Ok)`.
4. **Validate behavior** — Perform example-specific validation (see below).
5. **Collect logs** — Run `ank logs <workload_name>` to capture workload output.
6. **Cleanup** — Run `examples/cleanup.sh` after each example.

### Example-specific validation

#### Control Interface examples (`rust_control_interface`, `cpp_control_interface`, `nodejs_control_interface`, `python_control_interface`)

After the example workload reaches `Running(Ok)`:

- Wait up to 30 seconds for the `dynamic_nginx` workload to appear in `ank get workloads`.
- Verify `dynamic_nginx` reaches `Running(Ok)`.
- Check example workload logs (`ank logs <example_name>`) for evidence of successful Control Interface communication (e.g., workload state output).
- Mark as **passed** if `dynamic_nginx` was created and reached Running state.

#### `python_sdk_hello` / `rust_sdk_hello`

After the example workload reaches `Running(Ok)`:

- Check workload logs for evidence of:
  1. `dynamic_nginx` workload being created.
  2. `dynamic_nginx` reaching a Running state.
  3. `dynamic_nginx` being deleted.
- The example workload may exit after completion — this is expected.
- Mark as **passed** if logs show the create → wait → delete lifecycle completed.

#### `python_sdk_interactive`

After the example workload reaches `Running(Ok)`:

- This example just sleeps; there is no automated validation beyond reaching `Running(Ok)`.
- Mark as **passed** if the workload is running.

#### `python_sdk_logging` / `rust_sdk_logging`

After the example workload reaches `Running(Ok)`:

- Wait for the `screamer` workload (defined in the same manifest) to also reach `Running(Ok)`.
- Check example workload logs for output containing "ANKAIOS" (the text the screamer outputs).
- Mark as **passed** if the example captured at least one log line from the screamer.

## Build timeout

Container image builds can take a long time, especially for Rust examples. Use a generous timeout (up to 10 minutes) for each build step. If a build times out, mark it as **failed (build timeout)** and move on.

## Retry policy

- Do **not** retry failed builds automatically.
- If a workload fails to reach `Running(Ok)` within the timeout, check `ank get workloads` output and agent/server logs for diagnostics before marking as failed.

## Error diagnostics

When a failure occurs, collect:

- `ank get workloads` output
- `ank get state` output (if workload did not appear at all)
- Example workload logs: `ank logs <workload_name>`
- Server logs: `tail -50 /tmp/ankaios-server.log`
- Agent logs: `tail -50 /tmp/ankaios-agent_A.log`

## Live run feedback

While running, continuously provide concise status updates:

- Current example (N of total)
- Current step (build / start / apply / validate / cleanup)
- Pass/fail decision with short reason

## Final report format

After all examples have been checked:

1. **Summary table** with columns: Example | Build | Deploy | Validate | Result
2. For each **failure**, include:
   - Exact step that failed
   - Command executed
   - Output (or output tail if long)
   - Likely cause
3. For each **success**, include:
   - Confirmation of key validation points (e.g., "dynamic_nginx created and reached Running")
4. **Overall result**: X/Y examples passed
5. End with: ask whether to investigate or fix any detected problems.

## Important notes

- The `run_example.sh` script handles the full build-and-deploy lifecycle — always use it rather than manually running podman build + Ankaios start + ank apply.
- Use `cleanup.sh` between examples to ensure clean state.
- SDK examples (python_sdk_*, rust_sdk_*) may download SDK packages from the internet during build. Ensure network access is available.
- The `python_sdk_interactive` example is inherently manual — only verify it starts successfully.
- The `admission_hooks` example requires special server configuration and should be skipped unless explicitly requested.
- Always use `--insecure` or the `-k` flag when calling `ank` commands in the devcontainer (the `ank` alias already includes this).
