---
description: Load the current cotext packet for this repository.
---

Run `cotext render --audience claude` from the project root and treat the result as the authoritative design/notes/progress/todo context for `cotext`.

Then:

1. If the user is asking what to do next or to continue ongoing work, also run `cotext list --category todo` and `cotext list --category deferred`.
2. If only one slice matters, narrow with `cotext render --category <category> --audience claude`, `cotext list --category <category>`, or `cotext show <id>`.
3. Summarize the active items you are going to follow before you proceed with implementation.
