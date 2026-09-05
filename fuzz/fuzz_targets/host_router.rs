#![no_main]

use libfuzzer_sys::fuzz_target;
use scety::host_router::HostRouter;

fuzz_target!(|input: &str| {
    let mut router = HostRouter::new();
    router.add_pattern("example.com", 0);
    router.add_pattern("*.example.com", 1);
    router.add_pattern("**.internal.example.com", 2);
    router.add_pattern("api.*.example.com", 3);
    let _ = router.matches(input);
});
