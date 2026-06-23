
<!-- BACKLOG.MD MCP GUIDELINES START -->

<CRITICAL_INSTRUCTION>

## BACKLOG WORKFLOW INSTRUCTIONS

This project uses Backlog.md MCP for all task and project management activities.

**CRITICAL GUIDANCE**

- If your client supports MCP resources, read `backlog://workflow/overview` to understand when and how to use Backlog for this project.
- If your client only supports tools or the above request fails, call `backlog.get_backlog_instructions()` to load the tool-oriented overview. Use the `instruction` selector when you need `task-creation`, `task-execution`, or `task-finalization`.

- **First time working here?** Read the overview resource IMMEDIATELY to learn the workflow
- **Already familiar?** You should have the overview cached ("## Backlog.md Overview (MCP)")
- **When to read it**: BEFORE creating tasks, or when you're unsure whether to track work

These guides cover:
- Decision framework for when to create tasks
- Search-first workflow to avoid duplicates
- Links to detailed guides for task creation, execution, and finalization
- MCP tools reference

You MUST read the overview resource to understand the complete workflow. The information is NOT summarized here.

</CRITICAL_INSTRUCTION>

<!-- BACKLOG.MD MCP GUIDELINES END -->

## Code & comment conventions

**Never reference issue-tracker or plan artifacts in code, comments, or docstrings.**
Backlog task IDs (`TASK-9`, `task-10.1`), PRD slice numbers (`slice 4`), and milestone tags
(`S1`–`S8`) are *tracking* metadata: they belong in Backlog tasks and commit messages, not in
the source tree. A comment must explain **what the code does and why** — that stays true as
tickets come and go — whereas a `(task-10.3)` or `(S3)` tag is noise that rots and means
nothing to a future reader of the file.

- ❌ `//! Delivery decision (S3).`  ·  `// bind the target sink (task-9)`  ·  `// slice 4 wires this`
- ✅ `//! Delivery decision.`  ·  `// bind the target sink at trigger time`

When a comment needs to cite rationale, link a **durable design doc** instead — these are
encouraged: `CONTEXT.md` (domain language), `IDEAS.md` (design notes), and `docs/adr/` /
`ADR-NNNN` (decision records). Reference the *concept or decision*, never the ticket that
introduced it.

This covers every comment kind (module docs, struct/field docstrings, inline comments) in all
crates. The ticket linkage lives in the Backlog task and the commit message, where it has a home.
