# scheme-downgrade

An https start URL redirects to `http://www.example.gov/`. The www twin is the same site, but the scheme is a downgrade, so the origin is not added to the scope.

Checks: the start URL is `unreachable: off_scope_redirect`, the run reports a start failure, and the limits contain `redirect_scheme_downgrade_not_followed: http://www.example.gov/`.
