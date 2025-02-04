extern crate spider;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::spider::tokio::io::AsyncWriteExt;
use spider::string_concat::{string_concat, string_concat_impl};
use spider::tokio;
use spider::website::Website;

static GLOBAL_URL_COUNT: AtomicUsize = AtomicUsize::new(0);

#[tokio::main]
async fn main() {
    let mut website: Website = Website::new("https://rsseau.fr/en")
        .with_respect_robots_txt(true)
        .with_user_agent(Some("SpiderBot"))
        .with_ignore_sitemap(true) // ignore running the sitemap on base crawl/scape methods. Remove or set to true to include the sitemap with the crawl.
        .with_sitemap(Some("/sitemap/sitemap-0.xml"))
        .build()
        .unwrap();

    let start = std::time::Instant::now();

    website.crawl_sitemap().await;

    website.persist_links();

    website.scrape_smart().await;

    let duration = start.elapsed();

    let links = website.get_all_links_visited().await;

    // for link in links.iter() {
    //     println!("- {:?}", link.as_ref());
    // }

    {
        use std::io::{stdout, Write};
        let mut lock = stdout().lock();

        for page in website.get_pages().unwrap().iter() {
            let separator = "-".repeat(page.get_url().len());
            let markdown = html2md::rewrite_html(&page.get_html(), true);
            writeln!(
                lock,
                "{}\n{}\n\n{}\n\n{}",
                separator,
                page.get_url(),
                // page.get_html(),
                markdown,
                separator
            )
            .unwrap();
        }
    }

    println!(
        "Time elapsed in website.crawl() is: {:?} for total pages: {:?}",
        duration,
        links.len()
    )
}
