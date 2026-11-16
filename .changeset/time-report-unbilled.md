---
"eimer": minor
---

`eimer time report --project` shows records without the invoiced flag ("Abgr.") in time as "nicht abgerechnet": they stay in the list, greyed out with a small "nicht abgerechnet" badge, and count apart from the billed on-site and remote totals. The `time-report` template takes `billed: false` per entry.
