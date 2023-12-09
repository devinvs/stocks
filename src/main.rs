use std::collections::HashMap;
use std::env;
use std::fs::File;

use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

use futures::future::join_all;
use std::io::Read;

use toml::Table;

#[derive(Debug, Deserialize)]
struct Account {
    name: String,
    stocks: Vec<Stock>,
}

#[derive(Debug, Deserialize)]
struct Stock {
    symbol: String,
    amount: f64,
    cost_basis: f64,
}

#[tokio::main]
async fn main() {
    let path = format!("{}/.local/share/stocks.toml", env::var("HOME").unwrap());
    let accounts = parse_accounts(&path);

    let mut stock_info = HashMap::new();

    for acct in accounts.iter() {
        for stock in acct.stocks.iter() {
            stock_info.insert(stock.symbol.clone(), (0.0, 0.0));
        }
    }

    let stock_info = update_stock_info(stock_info).await;

    print(&accounts, &stock_info);
}

fn parse_accounts(path: &str) -> Vec<Account> {
    let mut f = File::open(path).unwrap();
    let mut buf = String::new();

    f.read_to_string(&mut buf).unwrap();
    let t = buf.parse::<Table>().unwrap();

    let mut accts = vec![];

    for (name, val) in t.iter() {
        let mut stocks = vec![];

        for (stock_name, info) in val.as_table().unwrap().iter() {
            let amount = info.get("num").unwrap().as_float().unwrap();
            let cost_basis = info.get("price").unwrap().as_float().unwrap();

            stocks.push(Stock {
                symbol: stock_name.clone(),
                amount,
                cost_basis,
            })
        }

        accts.push(Account {
            stocks,
            name: name.clone(),
        });
    }

    accts
}

async fn update_stock_info(info: HashMap<String, (f64, f64)>) -> HashMap<String, (f64, f64)> {
    let futures = info.into_iter().map(|(symbol, _)| {
        tokio::spawn(async move {
            match get_nasdaq_value(&symbol, "stocks").await {
                Some(x) => (symbol, x),
                None => (
                    symbol.clone(),
                    get_nasdaq_value(&symbol, "etf").await.unwrap_or_default(),
                ),
            }
        })
    });

    join_all(futures)
        .await
        .into_iter()
        .map(|res| res.unwrap())
        .collect()
}

async fn get_nasdaq_value(symbol: &str, class: &str) -> Option<(f64, f64)> {
    let client = Client::new();
    let url = format!(
        "https://api.nasdaq.com/api/quote/{}/info?assetclass={}",
        symbol, class
    );

    let res = client.get(url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/85.0.4183.121 Safari/537.36")
            .header("Accept", "*/*")
            .header("Accept-Encoding", "gzip, deflate, br")
            .header("Connection", "keep-alive")
            .send().await.ok()?
            .text().await.ok()?;

    let v: Value = serde_json::from_str(&res).ok()?;
    let price_str = v["data"]["primaryData"]["lastSalePrice"].as_str()?;
    let change_str = v["data"]["primaryData"]["netChange"].as_str()?;

    let price = price_str[1..].parse::<f64>().ok()?;
    let change = change_str.parse::<f64>().ok()?;

    Some((price, change))
}

fn clr(f: f64) -> String {
    if f < 0.0 {
        "\x1b[38;5;1m"
    } else {
        "\x1b[38;5;2m"
    }
    .to_string()
}

fn print(accounts: &Vec<Account>, stock_info: &HashMap<String, (f64, f64)>) {
    for account in accounts {
        println!("{}:", account.name);
        println!("\x1b[1m\tSymbol\t  Price      Net     Net %      Total   Total %\x1b[0m");

        for stock in account.stocks.iter() {
            let name = &stock.symbol;
            let (price, net) = stock_info[name];

            let old = price + net;
            let net_perc = (old - price) * 100.0 / old;

            let total_net = (price - stock.cost_basis) * stock.amount;
            let old = stock.cost_basis * stock.amount;
            let new = price * stock.amount;

            let total_perc = (new - old) * 100.0 / old;

            let gain = net * stock.amount;

            println!("\t{}\t${:>7.2}  {}${:>6.2}\x1b[0m  {}{:>6.2}%\x1b[0m  {}${:>9.2}\x1b[0m  {}{:>6.2}%\x1b[0m",
                     name,
                     price,
                     clr(gain),
                     gain,
                     clr(net_perc),
                     net_perc,
                     clr(total_net),
                     total_net,
                     clr(total_perc),
                     total_perc,
            );
        }
    }
}
