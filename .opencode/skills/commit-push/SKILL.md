---
name: commit-push
description: Use when the user explicitly asks to commit and push changes directly to the main branch without creating a pull request.
---

# Commit and Push to Main

Use this workflow only when the user explicitly requests a commit and push.

1. Confirm that the current branch is `main`. If it is not, do not commit or push; report the branch mismatch.
2. Inspect `git status`, the intended diff, and the last ten commits before staging changes.
3. Run the relevant verification commands.
4. Stage only the files intended for the commit.
5. Create a concise English commit message that describes the committed changes.
6. Do not create a pull request. Push the commit directly with `git push origin main`.
7. Report the commit hash and a minimal summary of the committed changes.

Do not amend commits, force-push, or include unrelated changes unless the user explicitly requests it.
