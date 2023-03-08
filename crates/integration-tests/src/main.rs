#[cfg(test)]
mod kumod;

fn main() {
    println!("Run me via `cargo nextest run` or `cargo test`");
}

#[cfg(test)]
mod test {
    use super::kumod::*;
    use mailparse::MailHeaderMap;
    use rfc5321::*;
    use std::time::Duration;

    #[tokio::test]
    async fn end_to_end() -> anyhow::Result<()> {
        eprintln!("start sink");
        let mut sink = KumoDaemon::spawn_maildir().await?;
        eprintln!("start source");
        let smtp = sink.listener("smtp");
        let mut source = KumoDaemon::spawn(KumoArgs {
            policy_file: "source.lua".to_string(),
            env: vec![("KUMOD_SMTP_SINK_PORT".to_string(), smtp.port().to_string())],
        })
        .await?;

        eprintln!("sending message");
        let mut client = source.smtp_client().await?;
        client.ehlo("localhost").await?;
        const BODY: &str =
        "From: <me@localhost>\r\nTo: <you@localhost>\r\nSubject: a test message\r\n\r\nAll done";
        let response = client
            .send_mail(
                ReversePath::try_from("sender@example.com").unwrap(),
                ForwardPath::try_from("recipient@example.com").unwrap(),
                BODY,
            )
            .await?;
        eprintln!("{response:?}");
        drop(client);

        let md = sink.maildir();
        eprintln!("waiting for maildir to populate");
        while md.count_new() < 1 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let stop_1 = source.stop();
        let stop_2 = sink.stop();

        let mut messages = vec![];
        for entry in md.list_new() {
            messages.push(entry?);
        }

        assert_eq!(messages.len(), 1);
        let parsed = messages[0].parsed()?;
        println!("headers: {:?}", parsed.headers);

        assert!(parsed.headers.get_first_header("Received").is_some());
        assert!(parsed.headers.get_first_header("X-KumoRef").is_some());
        assert_eq!(
            parsed.headers.get_first_value("From").unwrap(),
            "<me@localhost>"
        );
        assert_eq!(
            parsed.headers.get_first_value("To").unwrap(),
            "<you@localhost>"
        );
        assert_eq!(
            parsed.headers.get_first_value("Subject").unwrap(),
            "a test message"
        );
        assert_eq!(parsed.get_body()?, "All done\r\n");

        let (res_1, res_2) = tokio::join!(stop_1, stop_2);
        res_1?;
        res_2?;
        println!("Stopped!");

        Ok(())
    }
}
