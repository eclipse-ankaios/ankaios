---
name: tutorial-checker
description: Check Ankaios tutorials by executing safe documented shell steps and validating expected outputs. Use this when asked to "check tutorials on main" or to verify local tutorial files.
---

# Tutorial Checker Skill

Use this skill to validate tutorials by directly following tutorial steps as the agent (not by delegating logic to a large checker script).

## Trigger behavior

- If the user asks "check tutorials on main" (or equivalent), use main docs as default without asking for source.
- If the user asks to check local tutorials, ask for the tutorial name (for example `events` for `tutorial-events.md`).

## Tutorial discovery

- Main tutorials:
    - Load `https://eclipse-ankaios.github.io/ankaios/main/usage/installation/`.
    - Discover all links containing `/usage/tutorial-` at runtime.
- Local tutorials:
    - Discover files matching `/workspaces/ankaios/doc/docs/usage/tutorial-*.md` at runtime.
    - If user requested local mode without a name, ask for the name.

## Temporary workspace (required)

- If you need to create tutorial-generated files in the repository workspace, use a dedicated folder under `target` (for example `target/tutorial-check-outputs`) and clean it up at the end.
- Create all required files and intermediate files inside the dedicated folder
- Run file-dependent commands from that temp directory (or use absolute temp paths) so no accidental repo files are produced.
- At the end, report the temp directory used and whether cleanup was performed.

## Execution process (agent-driven)

For each selected tutorial:

1. Parse markdown and extract shell code blocks.
2. Split multiline shell blocks into executable commands (respect line continuations using `\`).
3. Run reach step one by one.
4. For each command:
    - Show progress before execution (`Tutorial X/Y - Step A/B: <command>`).
    - Classify step as `execute`, `transform-and-execute`, `skip-manual`, or `skip-unsafe`.
    - Systemd is not available inside the devcontainer, however if you use ankaios-start script that does not accept a startup manifest, just use ankaios-start and start on the manifest content dependent Ankaios agents additionally and apply the startup manifest of the tutorial with a simple `ank apply` instead. Classify the step as `transform-and-execute` and report the transformation in the final report.
    - Execute safe commands in terminal and capture stdout/stderr + exit code.
    - If a waiting time is required before the next command, don't combine it with the next command as this may affect allowed command detection and execution.
5. Validate expectations:
    - If surrounding text states "should print" and a following text block exists, compare output to expected snippets.
    - Mark step as failed if command fails or expected snippets are missing.
    - Include a warning in the final report as this may indicate a problem with the tutorial or environment.
6. Validate human understandability:
    - Review the tutorial text around each step and mark `understandable` or `unclear`.
    - Flag unclear items such as missing prerequisites, unexplained placeholders, ambiguous wording, missing expected result, or required manual context not stated.

## Command details

- The following commands are in the PATH:
    - `ank`, `ank-agent`, `ankaios-start`, `ankaios-clean`, `podman`, `systemctl`
- If the checking is running in a container, `systemctl` commands for starting Ankaios shall be supplemented with `ankaios-start` and `ank-agent`. In case `systemdctl` was not tested, the user must be informed in the final report.
- Disallow chained commands (for example `cmd1; cmd2`, `cmd1 && cmd2`, `cmd1 || cmd2`).
- Transform streaming commands to non-blocking equivalents and execute the transformed command:
    - `ank logs -f <workload>` or `ank logs --follow <workload>` -> `ank logs --tail 10 <workload>`
    - `mosquitto_sub ...` -> bounded receive mode, e.g. add `-C <count>` and `-W <seconds>` so command exits automatically.
    - Report both original and transformed command in step output.
- When server server IP substitution is required (for example containing `<SERVER_IP>`) use `http://localhost:25551` and assume all execution is local.

## Web form simulation (curl)

Some tutorials instruct the user to open a browser and interact with a web UI (e.g., enter a speed value).
The agent can automate this with `curl`, but only under strict conditions:

### Activation (user confirms via chat UI)

- When the agent encounters a web UI step, it executes the curl command in the terminal like any other command.
- The VS Code chat UI will show the command to the user for approval before execution — the user accepts or rejects it directly in the UI.
- No special flags or keywords are needed.

### Scope restrictions (mandatory)

All of the following must be true before the agent issues any curl command:

1. **Localhost only** — target must be `127.0.0.1` or `localhost`. Any other host is forbidden.
2. **Port from tutorial** — the port number must appear in the tutorial text itself (e.g., `http://127.0.0.1:5000`). Never guess or scan for ports.
3. **Workload must be running** — the workload serving the web UI must have been started by the agent during this tutorial run and confirmed as `Running(Ok)`.
4. **Form discovery** — before POSTing, fetch the page with `curl -s <url>` to extract the HTML form action and field names. Never invent field names.
5. **No external requests** — curl must never target external URLs, APIs, or services outside the tutorial's locally running workloads.

### Execution flow

When a tutorial step says something like "open browser at `<url>`" or "use the web UI to enter a value":

1. Fetch the page HTML: `curl -s <url>`
2. Parse the `<form>` element to determine: method, action path, and input field `name` attributes.
3. Submit a reasonable test value: `curl -s -X POST <url><action> -d "<field>=<value>"`
4. Verify the response contains a success indicator (e.g., confirmation text in HTML).
5. If a consumer workload should receive the value, check its logs to confirm end-to-end delivery.
6. Classify the step as `transform-and-execute` and report both the original instruction and the curl commands used.

### Reporting

In the final report, list all curl commands executed under a separate **"Web form simulation"** subsection, including:

- the original tutorial instruction
- the curl discovery and submission commands
- the response summary
- whether end-to-end verification passed

## Live run feedback

- While running, continuously provide concise status updates:
    - current tutorial
    - current step
    - pass/fail/skip decision
    - short reason for skips/failures

## Final report format

After all selected tutorials:

1. Per tutorial: `passed`, `failed`, `skipped` counts.
2. For each failure include:
    - exact command
    - output (or output tail if long)
    - expected snippets (if any)
    - likely cause / what may have gone wrong
3. Human-understandability summary per tutorial:
    - only list `unclear` steps with their surrounding text and reason for being unclear
    - give a brief overall assessment of the tutorial's clarity and usability based on the number and severity of unclear steps
4. End with: ask whether to try fixing detected problems.
