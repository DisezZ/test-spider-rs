extern crate spider;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::spider::http_cache_reqwest::CacheManager;
use crate::spider::tokio::io::AsyncWriteExt;
use spider::string_concat::{string_concat, string_concat_impl};
use spider::tokio;
use spider::website::Website;

static GLOBAL_URL_COUNT: AtomicUsize = AtomicUsize::new(0);

// #[tokio::main]
// async fn main() {
//     let mut website: Website = Website::new("https://rsseau.fr/en")
//         .with_respect_robots_txt(true)
//         .with_user_agent(Some("SpiderBot"))
//         .with_ignore_sitemap(true) // ignore running the sitemap on base crawl/scape methods. Remove or set to true to include the sitemap with the crawl.
//         .with_sitemap(Some("/sitemap/sitemap-0.xml"))
//         .build()
//         .unwrap();
//
//     let start = std::time::Instant::now();
//
//     website.crawl_sitemap().await;
//
//     website.persist_links();
//
//     website.scrape_smart().await;
//
//     let duration = start.elapsed();
//
//     let links = website.get_all_links_visited().await;
//
//     // for link in links.iter() {
//     //     println!("- {:?}", link.as_ref());
//     // }
//
//     {
//         use std::io::{stdout, Write};
//         let mut lock = stdout().lock();
//
//         for page in website.get_pages().unwrap().iter() {
//             let separator = "-".repeat(page.get_url().len());
//             let markdown = html2md::rewrite_html(&page.get_html(), true);
//             writeln!(
//                 lock,
//                 "{}\n{}\n\n{}\n\n{}",
//                 separator,
//                 page.get_url(),
//                 // page.get_html(),
//                 markdown,
//                 separator
//             )
//             .unwrap();
//         }
//     }
//
//     println!(
//         "Time elapsed in website.crawl() is: {:?} for total pages: {:?}",
//         duration,
//         links.len()
//     )
// }

#[tokio::main]
async fn main() {
    let mut website: Website = Website::new("https://www.heygoody.com/")
        .with_caching(true)
        .build()
        .unwrap();
    let mut rx2 = website.subscribe(500).unwrap();

    let start = std::time::Instant::now();

    tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();

        while let Ok(res) = rx2.recv().await {
            let message = format!("CACHING - {:?}\n", res.get_url());
            let _ = stdout.write_all(message.as_bytes()).await;
        }
    });

    website.crawl().await;
    website.unsubscribe();

    let mut rx2 = website.subscribe(500).unwrap();

    let subscription = async move {
        while let Ok(res) = rx2.recv().await {
            let mut stdout = tokio::io::stdout();
            let cache_url = string_concat!("GET:", res.get_url());

            tokio::task::spawn(async move {
                let result = tokio::time::timeout(Duration::from_millis(60), async {
                    spider::website::CACACHE_MANAGER.get(&cache_url).await
                })
                .await;

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

                GLOBAL_URL_COUNT.fetch_add(1, Ordering::Relaxed);
            });
        }
    };

    let crawl = async move {
        website.crawl_raw().await;
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
