# cookielessaudiences, a Rust client

Async Rust access to cookieless audience profiles and IAB categories for any page URL. No cookies, no identifiers, no personal data.

Returns `serde_json::Value`, so you decide how much of the response to model.

## Cargo

```toml
[dependencies]
cookielessaudiences = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

TLS is rustls, so there is no OpenSSL to link.

## Smallest working program

```rust
use cookielessaudiences::Client;

#[tokio::main]
async fn main() -> Result<(), cookielessaudiences::Error> {
    let client = Client::new(std::env::var("COOKIELESS_KEY").unwrap());
    let page = client.segment("https://example.com/blog").await?;

    println!("type: {}", page["audience_type"]);
    println!("age:  {}", page["demographics"]["age_bracket"]);
    Ok(())
}
```

## What is on the client

| Function | Purpose |
|---|---|
| `Client::new(key)` | builds a client with a 120 second timeout |
| `segment(url)` | structured v2 audience profile |
| `segment_legacy(url)` | the older free-text shape |
| `categorize(url)` | IAB v3 and v2 categories with confidence |
| `categorize_text(text)` | IAB categories for plain text |
| `Client::vocabularies()` | all allowed values, no key |
| `labels_for(&value)` | readable names for `INT.*` and `PI.*` codes |

## The error type

```rust
use cookielessaudiences::Error;

match client.segment(url).await {
    Ok(v) => println!("{v}"),
    Err(Error::Api { status: 403, .. }) => eprintln!("credits used up or key inactive"),
    Err(Error::Api { status, message, .. }) => eprintln!("{status}: {message}"),
    Err(Error::Transport(e)) => eprintln!("network problem: {e}"),
}
```

`Error::Api` keeps the whole response body, which is handy when you want to log the exact payload a support ticket needs.

## Fan out with a bounded stream

Cargo gains `futures = "0.3"` for this one.

```rust
use futures::{stream, StreamExt};

let urls = vec!["https://a.example", "https://b.example", "https://c.example"];
let client = std::sync::Arc::new(client);

let results: Vec<_> = stream::iter(urls)
    .map(|u| {
        let c = client.clone();
        async move { (u, c.segment(u).await) }
    })
    .buffer_unordered(8)
    .collect()
    .await;

for (url, res) in results {
    match res {
        Ok(v) => println!("{url}: {}", v["audience_type"]),
        Err(e) => println!("{url}: {e}"),
    }
}
```

Eight at a time is gentle. The service tolerates far more when you need throughput.

## Use it for brand safety and fit

Teams that check whether a publisher matches an advertiser read the same fields. Combine `content_context` with `interests` and you have the raw material for [brand fit analysis](https://www.cookielessaudiences.com/use-cases/brand-fit-analysis.php) without profiling a single person.

## Use it for IAB packaging

`categorize()` returns IAB v3 with 703 categories and v2 with 698 categories. Read the [IAB audience taxonomy 1.1](https://www.cookielessaudiences.com/features/iab-audience-taxonomy.php) page to see how interests and purchase intent line up with the standard.

## Pitfalls

- The audience endpoint takes form-encoded POST, not JSON. The client handles that.
- Subdomains with thin content return status 410 unless you pass a root domain yourself.
- Status lives in the body. Do not rely on the HTTP code.
- Do not call from a browser or mobile app; keep the key on a server.

## Questions

**Why `Value` and not structs?** The response gains fields as vocabularies grow. A `Value` keeps your build green.

**Blocking version?** Run the future with `tokio::runtime::Runtime::block_on`.

**License?** MIT.

Contact: info@alpha-quantum.com
