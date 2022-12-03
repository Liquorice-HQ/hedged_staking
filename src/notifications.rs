use flexi_logger::*;
use flexi_logger::writers::*;
use termion::{color, style};

use crate::config::NotificationsConfig;


pub struct TelegramLogWriter {
    pub config: NotificationsConfig,
}


fn limit_str(s: &String) -> String {
    let limit: usize = 2048;
    if s.len() > limit {
        format!("{} ...", &s[0..(limit - 1)])
    }
    else {
        s.to_owned()
    }
}


impl LogWriter for TelegramLogWriter {
    fn write(&self, now: &mut DeferredNow, record: &Record<'_>) -> std::io::Result<()> {
        // TODO make delayed send
        if self.config.telegram_enabled &&
           (record.level() <= Level::Warn || (record.level() <= Level::Info && record.target() == "NOTIFICATION")) {
                let raw_msg = limit_str(&record.args().to_string());
                let msg = html_escape::encode_text(&raw_msg).to_string();
                let logmsg = match record.level() {
                    Level::Warn => format!("<b>Warning: {}</b>", msg),
                    Level::Error => format!("<b>ERROR: {}</b>", msg),
                    _ => msg.to_owned(),
                };
                send_message(
                    format!("{}\n\n<i>{}</i>",
                            logmsg,
                            now.now()),
                    &self.config.logs_telegram_token,
                    self.config.logs_telegram_chat_id,
                    true);
                if record.level() <= Level::Warn {
                    let place:String = match (record.file_static(), record.line()) {
                        (None, None) => String::new(),
                        (None, Some(line)) => format!("???:{}", line),
                        (Some(file), None) => file.to_owned(),
                        (Some(file), Some(line)) => format!("{}:{}", file, line),
                    };
                    let logmsg = match record.level() {
                        Level::Error => format!("<b>ERROR: {}</b>", msg),
                        _ => msg,
                    };
                    send_message(
                        format!("{}\n\n<code>{}</code>\n<i>{}</i>",
                                logmsg,
                                place,
                                now.now()),
                        &self.config.alerts_telegram_token,
                        self.config.alerts_telegram_chat_id,
                        false);
            }
        }
        Ok(())
    }
    fn flush(&self) -> std::io::Result<()> {
        Ok(())
    }

    //fn max_log_level(&self) -> LevelFilter { return flexi_logger::LevelFilter::Trace; }
    //fn format(&mut self, format: FormatFunction) { unimplemented!() }
    //fn shutdown(&self) { () }
}


// From:
// https://docs.rs/telegram_notifyrs/latest/src/telegram_notifyrs/lib.rs.html
//

/// Sends a Telegram message
///
/// Sends the supplied message to the designated chad ID, using the supplied token.
pub fn send_message(msg: String, token: &str, chat_id: i64, is_silent: bool) {
    match ureq::post(&format!(
        "https://api.telegram.org/bot{token}/sendMessage",
        token = &token
    ))
    .send_json(ureq::json!({
        "text": msg,
        "chat_id": chat_id,
        "parse_mode": "html",
        "disable_notification": !is_silent,
    })) {
        Ok(_) => (),
        Err(err) => {
            println!("{}Can't notify Telegram{}", color::Fg(color::Red), style::Reset);
            println!("{}Error message: {:?}{}", color::Fg(color::Red), err, style::Reset);
            println!("{}Notify message: {}{}", color::Fg(color::Red), msg, style::Reset);
        }
    }
}

