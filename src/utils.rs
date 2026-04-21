pub fn time_ago_in_words(duration: chrono::Duration) -> String {
    let years = duration.num_weeks() / 52;
    let time_ago = if years > 0 {
        pluralize(years, "year", "years")
    } else {
        let months = duration.num_weeks() / 4;
        if months > 0 {
            pluralize(months, "month", "months")
        } else if duration.num_weeks() > 0 {
            pluralize(duration.num_weeks(), "week", "weeks")
        } else if duration.num_days() > 0 {
            pluralize(duration.num_days(), "day", "days")
        } else if duration.num_hours() > 0 {
            pluralize(duration.num_hours(), "hour", "hours")
        } else {
            let minutes = duration.num_minutes();
            if minutes == 0 {
                return "just now".to_string();
            } else {
                pluralize(minutes, "minute", "minutes")
            }
        }
    };
    format!("{time_ago} ago")
}

pub fn pluralize(number: i64, singular: &'static str, plural: &'static str) -> String {
    let label = if number == 1 { singular } else { plural };
    format!("{} {}", number, label)
}
