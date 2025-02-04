extern crate spider;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::spider::http_cache_reqwest::CacheManager;
use crate::spider::tokio::io::AsyncWriteExt;
use spider::features::chrome_common::RequestInterceptConfiguration;
use spider::string_concat::{string_concat, string_concat_impl};
use spider::tokio;
use spider::website::Website;
use spider_transformations::transformation::content;

static GLOBAL_URL_COUNT: AtomicUsize = AtomicUsize::new(0);

#[tokio::main]
async fn main() {
    let target = "https://www.heygoody.com/";
    // let target = "https://rsseau.fr/en";
    // // let target = "https://facebook.com";
    // // let target = "https://jeffmendez.com";
    let mut website = Website::new(target);

    website
        .with_respect_robots_txt(false)
        .with_user_agent(Some("SpiderBot"))
        .with_ignore_sitemap(true) // ignore running the sitemap on base crawl/scape methods. Remove or set to true to include the sitemap with the crawl.
        .with_sitemap(Some("/sitemap/sitemap-0.xml"))
        .with_caching(true)
        .with_chrome_intercept(RequestInterceptConfiguration::new(true))
        .with_stealth(true)
        .with_depth(50)
        .with_limit(500);

    let start = std::time::Instant::now();

    website.crawl_sitemap_chrome().await;

    let mut rx2 = website.subscribe(500).unwrap();

    let subscription = async move {
        while let Ok(res) = rx2.recv().await {
            let mut stdout = tokio::io::stdout();
            let cache_url = string_concat!("GET:", res.get_url());

            tokio::task::spawn(async move {
                let link = res.get_url();
                let result = tokio::time::timeout(Duration::from_millis(60), async {
                    spider::website::CACACHE_MANAGER.get(&cache_url).await
                })
                .await;

                let _ = stdout.write_all(b"\n\n#### ==== ####\n").await;
                match result {
                    Ok(Ok(Some(_cache))) => {
                        let message = format!("HIT - {:?}\n", cache_url);
                        let _ = stdout.write_all(message.as_bytes()).await;
                    }
                    Ok(Ok(None)) | Ok(Err(_)) => {
                        let message = format!("MISS - {:?}\n", cache_url);
                        let _ = stdout.write_all(message.as_bytes()).await;
                    }
                    Err(_) => {
                        let message = format!("ERROR - {:?}\n", cache_url);
                        let _ = stdout.write_all(message.as_bytes()).await;
                    }
                };

                // get html and parse it into markdown
                let markdown = parse_markdown(&res.get_html());
                let _ = stdout.write_all(markdown.as_bytes()).await;

                GLOBAL_URL_COUNT.fetch_add(1, Ordering::Relaxed);
            });
        }
    };

    let crawl = async move {
        website.crawl_smart().await;
        website.unsubscribe();
    };

    tokio::pin!(subscription);

    tokio::select! {
        _ = crawl => (),
        _ = subscription => (),
    };

    let duration = start.elapsed();

    println!(
        "Time elapsed in website.crawl() is: {:?} for total pages: {:?}",
        duration, GLOBAL_URL_COUNT
    )
}

fn parse_markdown(html: &String) -> String {
    // let markdown = html2md::rewrite_html(&res.get_html(), false);
    let markdown = content::transform_markdown(html, false);
    markdown
}
