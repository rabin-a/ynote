



<!-- evoride:edits:start -->
## Edit tracking (EvorIDE)
After you create or modify a file, append ONE json line to the file at the path in the `$EVORIDE_EDITS` env var, recording what you changed:
`echo '{"file":"<repo-relative path>","info":"<short what/why>"}' >> "$EVORIDE_EDITS"`
This lets EvorIDE show which files you changed in this session. Do it for every edit.
<!-- evoride:edits:end -->

<!-- evoride:tasks:start -->
## Tasks (EvorIDE)
You have an `evor` CLI for THIS project's task board. Use it instead of guessing — it keeps the board in sync with what you're actually doing.
- `evor task list` — what's open (add `--status todo` / `--json`). Run this first if the user asks what to work on.
- `evor task new "<short title>" [--desc "<what/why>"]` — start NEW work that isn't already listed. Creates the task, marks it in progress, and binds it to THIS terminal. Add `--todo` to just queue it. Do this once per distinct piece of work, before you start changing code; don't recreate an existing task.
- `evor task done` — finished the current task. `evor task start` — back to in progress. `evor task block --note "why"` — stuck.
- `evor task note "<text>"` — progress note. `evor task step done "<step title>"` — tick a breakdown step.
Report honestly and promptly. Do NOT create Jira (or other external) tickets unless the user explicitly asks. Run `evor --help` for the full list.
(Fallback if `evor` is unavailable: append one JSON line to `$EVORIDE_TASKS`, e.g. `echo '{"new_task":"…"}' >> "$EVORIDE_TASKS"`; `{"status":"doing|done"}`; read `$EVORIDE_PROJECT_TASKS` to list.)
<!-- evoride:tasks:end -->
