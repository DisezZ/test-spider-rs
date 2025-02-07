extern crate spider;

use core::str;
use std::collections::HashSet;
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

#[derive(Debug, Copy, Clone)]
enum CrawlerMode {
    HTTPReq,
    Chrome,
}

struct Crawler {
    target: String,
    mode: CrawlerMode,
}

impl Crawler {
    async fn new(target: String) -> Self {
        let mode = match determine_ssr_or_spa(target.as_str()).await {
            WebsiteType::SSR => CrawlerMode::HTTPReq,
            WebsiteType::SPA => CrawlerMode::Chrome,
        };
        Self { target, mode }
    }

    async fn start(&self) {
        let robot_sitemaps = self.get_sitemaps_from_robots().await;
        println!(
            "DEBUG: Sitemap From Robots.txt - {:?} {}",
            robot_sitemaps,
            robot_sitemaps.as_ref().unwrap().len()
        );
        match robot_sitemaps {
            Some(sitemaps) => self.crawl_with_sitemaps(sitemaps).await,
            None => self.crawl_without_sitemaps().await,
        };
    }

    async fn get_sitemaps_from_robots(&self) -> Option<Vec<String>> {
        let robots_path = self.target.clone() + "robots.txt";
        let res = reqwest::get(robots_path).await.unwrap();
        let robots_txt = res.text().await.unwrap();
        let sitemaps = robots_txt
            .lines()
            .filter(|line| line.contains("Sitemap: "))
            .map(|line| line.split_ascii_whitespace().last().map(str::to_string))
            .flatten()
            .collect::<Vec<_>>();
        match !sitemaps.is_empty() {
            true => Some(sitemaps),
            false => None,
        }
    }

    async fn crawl_with_sitemaps(&self, sitemaps: Vec<String>) {
        let sitemap_entries = get_sitemaps_from_robots_sitemap(sitemaps.clone()).await;
        println!(
            "DEBUG: Sitemap Entries - {:?} {}",
            sitemap_entries,
            sitemap_entries.len()
        );

        let mut binding = Website::new(self.target.as_str());
        let website = binding
            .with_respect_robots_txt(true)
            .with_user_agent(Some("SpiderBot"))
            .with_stealth(true);

        // update config based on crawler mode
        match self.mode {
            CrawlerMode::HTTPReq => {}
            CrawlerMode::Chrome => {
                website.with_chrome_intercept(RequestInterceptConfiguration::new(true));
            }
        };

        // crawl only for each sitemap
        for sitemap in &sitemaps {
            // get sitemap path eg. "/sitemap.xml"
            let sitemap_path =
                "/".to_string() + sitemap.split(self.target.as_str()).collect::<Vec<_>>()[1];
            let sitemap_path = sitemap_path.as_str();

            // set crawler to use current sitemap
            website.with_sitemap(Some(sitemap_path)).build().unwrap();

            // crawl sitemap only
            website.crawl_sitemap().await;

            // get all visited links
            let links = website.get_all_links_visited().await;

            // we iterate each link and get html ==parse==> markdown
            let mut stdout = tokio::io::stdout();
            for link in links {
                let _ = stdout.write_all(b"\n\n#### ==== ####\n").await;
                let html = reqwest::get(link.to_string())
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap();
                let _ = stdout
                    .write_all(format!("{:?} => {}\n", GLOBAL_URL_COUNT, &link).as_bytes())
                    .await;

                // get html and parse it into markdown
                let markdown = parse_html_to_markdown(&html);
                let _ = stdout.write_all(format!("{}\n", markdown).as_bytes()).await;

                // increment links encounter count
                GLOBAL_URL_COUNT.fetch_add(1, Ordering::Relaxed);
            }

            // // persist visited link for next crawl
            // website.persist_links();
        }
    }

    async fn crawl_without_sitemaps(&self) {
        let mut binding = Website::new(self.target.as_str());
        let website = binding
            .with_respect_robots_txt(true)
            .with_user_agent(Some("SpiderBot"))
            .with_stealth(true);

        // update config based on crawler mode
        match self.mode {
            CrawlerMode::HTTPReq => (),
            CrawlerMode::Chrome => {
                website.with_chrome_intercept(RequestInterceptConfiguration::new(true));
            }
        };
        website.build().unwrap();

        // first and foremost, we crawl the sitemap
        website.crawl_sitemap_chrome().await;

        let mut rx2 = website.subscribe(500).unwrap();

        let subscription = async move {
            while let Ok(res) = rx2.recv().await {
                let mut stdout = tokio::io::stdout();

                tokio::task::spawn(async move {
                    let _ = stdout.write_all(b"\n\n#### ==== ####\n").await;

                    let _ = stdout
                        .write_all(
                            format!("{:?} => {}\n", GLOBAL_URL_COUNT, res.get_url()).as_bytes(),
                        )
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
    }
}
// Implement a crawler that do these thing in order
// 1. Determine whether site is SPA or SSR and create crawler accordingly
// 2. Craw the sitemap first
// 3. Loop the link and parse from html to markdown
#[tokio::main]
async fn main() {
    let target = "https://www.heygoody.com/";

    let crawler = Crawler::new(target.into()).await;

    let start = std::time::Instant::now();

    crawler.start().await;

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

// indicate state of reading xml
//  whether we currently at sitemap index or sitemap entry
enum SitemapXMLState {
    SitemapIndex,
    SitemapEntry,
    Other,
}

use spider::quick_xml::{events::Event, Reader};
async fn get_sitemaps_from_robots_sitemap(sitemaps: Vec<String>) -> Vec<String> {
    let mut sitemap_urls: HashSet<String> = HashSet::new();
    for sitemap in sitemaps {
        let mut state = SitemapXMLState::Other;
        let mut to_read = false;
        let xml = reqwest::get(&sitemap).await.unwrap().text().await.unwrap();
        let mut reader = Reader::from_str(xml.as_ref());
        reader.config_mut().trim_text(true);

        loop {
            match reader.read_event().unwrap() {
                Event::Eof => break,
                Event::Start(e) => {
                    let name = e.name();
                    let name = str::from_utf8(name.as_ref()).unwrap();
                    match name.as_ref() {
                        "urlset" => {
                            sitemap_urls.insert(sitemap.clone());
                        }
                        "url" => state = SitemapXMLState::SitemapEntry,
                        "sitemap" => state = SitemapXMLState::SitemapIndex,
                        "loc" => to_read = true,
                        _ => state = SitemapXMLState::Other,
                    }
                }
                Event::Text(e) => {
                    let text = String::from_utf8(e.as_ref().into()).unwrap();
                    match (&state, &to_read) {
                        (SitemapXMLState::SitemapIndex, true) => {
                            sitemap_urls.insert(text.clone());
                            to_read = false;
                        }
                        (SitemapXMLState::SitemapEntry, true) => {
                            to_read = false;
                        }
                        _ => (),
                    }
                }
                _ => {}
            }
        }
    }
    sitemap_urls.into_iter().collect()
}

async fn get_url_entries_from_sitemap_entries(sitemap_entries: Vec<String>) -> Vec<String> {
    let mut url_links = vec![];
    for sitemap_entry in sitemap_entries {
        let mut state = SitemapXMLState::Other;
        let mut to_read = false;
        let xml = reqwest::get(sitemap_entry)
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let mut reader = Reader::from_str(xml.as_ref());
        reader.config_mut().trim_text(true);

        loop {
            match reader.read_event().unwrap() {
                Event::Eof => break,
                Event::Start(e) => {
                    let name = str::from_utf8(&e).unwrap();
                    match name.as_ref() {
                        "url" => state = SitemapXMLState::SitemapEntry,
                        "loc" => to_read = true,
                        _ => state = SitemapXMLState::Other,
                    }
                }
                Event::Text(e) => {
                    let text = String::from_utf8(e.as_ref().into()).unwrap();
                    match (&state, &to_read) {
                        (SitemapXMLState::SitemapEntry, true) => {
                            url_links.push(text);
                            to_read = false;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    url_links
}
