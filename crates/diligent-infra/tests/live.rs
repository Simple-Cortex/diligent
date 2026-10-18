//! Live network test. Set `DILIGENT_LIVE_URL` to an https origin you are allowed to
//! test, then run `cargo test -p diligent-infra -- --ignored`. Without the variable
//! the test prints a skip message and passes.

use diligent_infra::{run, DnsResolver, Options, ReqwestFetcher, Status};
use url::Url;

#[test]
#[ignore = "needs network access to the https origin in DILIGENT_LIVE_URL"]
fn live_origin_produces_rows_without_errors() {
    let Ok(target) = std::env::var("DILIGENT_LIVE_URL") else {
        eprintln!("skipped: DILIGENT_LIVE_URL unset");
        return;
    };
    let opts = Options::default();
    let fetcher = ReqwestFetcher::new(&opts).expect("fetcher builds");
    let resolver = DnsResolver::new(&opts).expect("resolver builds");
    let origin = Url::parse(&target).expect("DILIGENT_LIVE_URL is a valid URL");
    let report = run(&origin, &fetcher, &resolver, &opts);
    assert!(!report.rows.is_empty());
    assert!(report.rows.iter().all(|r| r.status != Status::Error));
}
