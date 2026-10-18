# process-incomplete

Two caller-supplied processes. `apply-permit` has three steps and the last one (`/apply/confirm`) answers 404, so the process is not complete, the missing step is listed, the run is partial and the reason is in `remaining_work`. `contact-us` is complete.

Every step that is a page of its own is a singleton template, so it enters the structured sample with reason `unique`; the first reason wins. Which pages belong to which process is recorded in `processes[].steps`.
