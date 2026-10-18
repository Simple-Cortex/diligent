# private-link-target

The operator allows the origin `http://10.0.0.5` (a mistake or a hostile hint), and the home page links to it. The address policy must still refuse it.

Checks: `http://10.0.0.5/admin` is a route with `unreachable: refused`, no cluster, is not in the sample, and is never requested (the fake fetcher's request log has no entry for it); `http://localhost:8080/x` is off scope and is not recorded as a vendor; the IDN host is recorded as its punycode form; `data:` links are ignored.
