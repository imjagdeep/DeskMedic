# Security

DeskMedic deletes files and changes disk settings with administrator rights, so safety bugs matter more than most.

## Reporting a problem

Please use GitHub's **private vulnerability reporting** (Security tab → Report a vulnerability) rather than a public issue. Include the Windows version, what you ran, and what happened. You'll get a reply within a week.

Especially welcome:
- any way to make Cleanup delete something outside its listed folders,
- any way to get user input into a command or script,
- any disk action that runs against the wrong disk or partition.

## How it is built to be safe

See "Safety rules" in the README. In short: a fixed list of cleanup folders behind a guard that refuses protected folders and never follows links; disk changes re-checked against a fresh read and confirmed by typing the disk number or letter; commands are fixed data with validated numbers passed as environment variables; everything that changes something is logged.
