I’m using the Orchestrate build instructions for this preflight probe, then I’ll create, stage, and commit only the requested file.

The requested file was created, but Git could not stage it because this fixture’s `.git/index` is not writable. I’m doing a final status check to confirm no other changes were made.

Commit did not succeed: `.git/index` is not writable, so the file remains untracked. No other changes were made.