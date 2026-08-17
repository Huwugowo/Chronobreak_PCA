# PC-B non-League verification state

## QB-PERF-002 attribution boundary

The QB-PERF-002 collector/analyzer under the handoff pack's
`01_CURRENT_REPO_REFERENCE` remains reference state. It has not been copied
into this native patch worktree or relabeled as QB-PERF-005 evidence. The three
independent heartbeat-free 240-second QB-PERF-002 captures still belong to the
untouched PC-A repository and are not complete.

Available static checks were run unchanged on PC B on 2026-08-17:

- all 60 `tools/capture_benchmark/tests` Python unit tests passed;
- all three capture-benchmark PowerShell scripts parsed without syntax errors;
- Intel PresentMon Console 2.5.1.0 was installed through WinGet, whose package
  hash verification passed.

The schema-v2 collector static preflight was then run with PresentMon 2.5.1.
It did not pass and therefore is not recorded as valid QB-PERF evidence:

```text
QB-PERF-CIM_UNAVAILABLE: Windows performance class
Win32_PerfRawData_PerfProc_Process is unavailable: invalid class
```

Read-only diagnosis found that the PerfProc performance library is enabled and
raw PerfOS classes work, but WMI exposes only PerfProc JobObject,
JobObjectDetails and Thread—not Process. The PC-B user token also lacks the
built-in Performance Monitor Users and Performance Log Users group SIDs. The
collector is intentionally unchanged: substituting a weaker counter source
would violate the common QB-PERF validity contract.

An administrator must restore counter access/registration and refresh the user
session before the unchanged preflight can be rerun. If group membership alone
does not restore the class, WMI performance classes require administrator-level
resynchronization. No League process is needed for that static preflight.

## QB-PERF-005 non-League state

M1-M6 and the non-League portion of M7 are implemented and verified in this
worktree. The strongest current lifecycle/performance evidence is documented in
`docs/M7_NATIVE_LIFECYCLE_EVIDENCE.md`. Actual League lifecycle acceptance and
M8 comparison remain external gates; this PC-B evidence does not substitute for
them.
