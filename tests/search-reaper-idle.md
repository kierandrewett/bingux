# Search process monitor checks

Run the bounded check without a compositor:

```sh
python3 tests/search-reaper-idle.py
```

The fixture compiles the production process monitor with Rust's standard
library. It checks idle task wakeups, new-child delivery, reaping, and shutdown
with a live child. It proves this worker's lifecycle, not desktop frame pacing.
