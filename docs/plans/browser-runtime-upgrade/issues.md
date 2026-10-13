# Issues

- BR-I1: Prior native CEF qualification observed a blank toolbar despite DOM readiness. Retained images prove startup presentation lag; the bounded wait is implemented without changing the pixel oracle. Owner: runtime-upgrade task. Disposition: fix implemented, BR4 remains blocking until native rerun.
- BR-I2: Active and standalone CEF adapters explicitly set `no_sandbox=1` in the base. This upgrade must not broaden bypasses; do not change system security settings to make tests pass. Owner: maintainers. Disposition: existing security debt, evaluate within runtime migration.
- BR-I3: Local executor lacks native system libraries/display qualification. Owner: runtime-upgrade task. Disposition: run independent checks and seek existing CI evidence; unavailable GUI acceptance remains explicit.

- BR-I4: Independent review found Bash conditional callers disable errexit, so a failed npm ci could be hidden by successful runtime bootstrap. Disposition: fixed with explicit && chaining and a regression proving bootstrap is skipped after npm ci failure.

- BR-I5: Review found missing Electron release runtime exited1 instead of the documented4. Disposition: fixed at launcher boundary, covered by regression.
- BR-I6: Reused Electron smoke incorrectly assumed sandbox preload process was global in isolated world999. Disposition: corrected to direct sandbox observation in the disposable preflight plus production preferences/preload/Node-global checks, with separate claims documented.
