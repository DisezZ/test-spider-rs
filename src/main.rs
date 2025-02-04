extern crate spider;

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::spider::tokio::io::AsyncWriteExt;
use spider::features::chrome_common::RequestInterceptConfiguration;
use spider::website::Website;
use spider::{reqwest, tokio};
use spider_transformations::transformation::content;

static GLOBAL_URL_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
enum WebsiteType {
    SSR,
    SPA,
}

#[derive(Debug)]
enum CrawlerMode {
    HTTPReq,
    Chrome,
}

// Implement a crawler that do these thing in order
// 1. Determine whether site is SPA or SSR and create crawler accordingly
// 2. Craw the sitemap first
// 3. Loop the link and parse from html to markdown
#[tokio::main]
async fn main() {
    let target = "https://www.heygoody.com/";

    // determine crawler mode from whether website is SPA or SSR
    let crawler_mode = match determine_ssr_or_spa(target).await {
        WebsiteType::SSR => CrawlerMode::HTTPReq,
        WebsiteType::SPA => CrawlerMode::Chrome,
    };
    println!("DEBUG: Crawler operate in {:?} mode", crawler_mode);

    // construct website struct based on crawler mode
    let mut binding = Website::new(target);
    let website = binding
        .with_respect_robots_txt(true)
        .with_user_agent(Some("SpiderBot"))
        .with_stealth(true);
    let website = match crawler_mode {
        CrawlerMode::HTTPReq => website,
        CrawlerMode::Chrome => {
            website.with_chrome_intercept(RequestInterceptConfiguration::new(true))
        }
    };
    let mut website = website.build().unwrap();
    println!("DEBUG: Crawler operate with {:?}", &website);

    // get start time
    let start = std::time::Instant::now();

    // first and foremost, we crawl the sitemap
    website.crawl_sitemap_chrome().await;

    let mut rx2 = website.subscribe(500).unwrap();

    let subscription = async move {
        while let Ok(res) = rx2.recv().await {
            let mut stdout = tokio::io::stdout();

            tokio::task::spawn(async move {
                let _ = stdout.write_all(b"\n\n#### ==== ####\n").await;

                let _ = stdout
                    .write_all(format!("{:?} => {}\n", GLOBAL_URL_COUNT, res.get_url()).as_bytes())
                    .await;

                // get html and parse it into markdown
                let markdown = parse_html_to_markdown(&res.get_html());
                let _ = stdout.write_all(format!("{}\n", markdown).as_bytes()).await;

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

// simple func to determine whtether a website is SSR or SPA, currently based on a simple
// factor, which is whether the initial response from the site is include script or not.
// currently:
// include => SPA
// !include => SSR
async fn determine_ssr_or_spa(url: &str) -> WebsiteType {
    let res = reqwest::get(url).await.unwrap();
    let html = res.text().await.unwrap();
    if html.contains("<script>") {
        WebsiteType::SPA
    } else {
        WebsiteType::SSR
    }
}

fn parse_html_to_markdown(html: &String) -> String {
    // use transform_markdown which is a sub-crate within the spider-rs, to transform html elements
    // into markdown text.
    content::transform_markdown(html, false)
}
