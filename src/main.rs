extern crate spider;

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::spider::tokio::io::AsyncWriteExt;
use spider::features::chrome_common::RequestInterceptConfiguration;
use spider::tokio;
use spider::website::Website;
use spider_transformations::transformation::content;

static GLOBAL_URL_COUNT: AtomicUsize = AtomicUsize::new(0);

// Implement a crawler that do these thing in order
// 1. Craw the sitemap first
// 2. Loop the link and parse from html to markdown
// And able to do all of those by auto determine whether to use
// HTTP request or Chrome headless rendering based on whether
// the website is SSR or SPA
#[tokio::main]
async fn main() {
    let target = "https://www.heygoody.com/";
    let mut website = Website::new(target)
        .with_respect_robots_txt(false)
        .with_user_agent(Some("SpiderBot"))
        // .with_ignore_sitemap(true) // ignore running the sitemap on base crawl/scape methods. Remove or set to true to include the sitemap with the crawl.
        .with_sitemap(Some("sitemap.xml"))
        .with_chrome_intercept(RequestInterceptConfiguration::new(true))
        .with_depth(10)
        .with_limit(100)
        .with_stealth(true)
        .build()
        .unwrap();

    let start = std::time::Instant::now();

    // first and foremost, we crawl the sitemap
    website.crawl_sitemap_chrome().await;

    let mut rx2 = website.subscribe(500).unwrap();

    let subscription = async move {
        while let Ok(res) = rx2.recv().await {
            let mut stdout = tokio::io::stdout();

            tokio::task::spawn(async move {
                let _ = stdout.write_all(b"\n\n#### ==== ####\n").await;

                let _ = stdout.write_all(res.get_url().as_bytes()).await;

                // get html and parse it into markdown
                let markdown = parse_html_to_markdown(&res.get_html());

                let _ = stdout.write_all(markdown.as_bytes()).await;

                GLOBAL_URL_COUNT.fetch_add(1, Ordering::Relaxed);
            });
        }
    };

    let crawl = async move {
        // crawl_smart state that it will use http first, and if applicable `javascript` rendering
        // is used if need.
        // what I understand is that it able to determine whether to use http request or chrome for
        // headless rendering. What I think will applied is it will use http request if website is SSR
        // and use chrome if it's SPA.
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

fn parse_html_to_markdown(html: &String) -> String {
    // use transform_markdown which is a sub-crate within the spider-rs, to transform html elements
    // into markdown text.
    content::transform_markdown(html, false)
}
