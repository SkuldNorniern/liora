use crate::runtime::{Heap, Value};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{number_to_value, to_number};

pub fn create(args: &[Value], heap: &mut Heap) -> Value {
    let ms = if args.len() >= 2 {
        let y = super::to_number(args.first().unwrap_or(&Value::Number(0.0))) as i32;
        let mo = super::to_number(args.get(1).unwrap_or(&Value::Number(0.0))) as i32;
        let d = args
            .get(2)
            .map(|v| super::to_number(v) as i32)
            .unwrap_or(1)
            .clamp(1, 31);
        let h = args.get(3).map(|v| super::to_number(v) as i32).unwrap_or(0);
        let m = args.get(4).map(|v| super::to_number(v) as i32).unwrap_or(0);
        let s = args.get(5).map(|v| super::to_number(v) as i32).unwrap_or(0);
        let ms_arg = args.get(6).map(super::to_number).unwrap_or(0.0);
        let mo_1_12 = (mo % 12 + 12) % 12 + 1;
        let days = ymd_to_days(y, mo_1_12, d);
        (days * 86400 + h as i64 * 3600 + m as i64 * 60 + s as i64) as f64 * 1000.0 + ms_arg
    } else {
        match args.first() {
            None => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as f64)
                .unwrap_or(0.0),
            Some(Value::Int(n)) => *n as f64,
            Some(Value::Number(n)) => *n,
            Some(v) => to_number(v),
        }
    };
    let id = heap.alloc_date(ms);
    Value::Date(id)
}

pub fn now(_args: &[Value], _heap: &mut Heap) -> Value {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0);
    number_to_value(ms)
}

pub fn get_time(args: &[Value], heap: &mut Heap) -> Value {
    let id = match args.first().and_then(Value::as_date_id) {
        Some(i) => i,
        None => return Value::Number(f64::NAN),
    };
    Value::Number(heap.date_timestamp(id))
}

fn format_date(ms: f64) -> String {
    if !ms.is_finite() {
        return "Invalid Date".to_string();
    }
    let secs = (ms / 1000.0) as i64;
    let millis = (ms % 1000.0) as i32;
    let days = secs / 86400;
    let t = secs % 86400;
    let h = t / 3600;
    let m = (t % 3600) / 60;
    let s = t % 60;
    let (y, mo, d) = days_to_ymd(days);
    format!(
        "{} {:02} {} {:04} {:02}:{:02}:{:02}.{:03} GMT",
        weekday_name((days + 4) % 7),
        d,
        month_name(mo),
        y,
        h,
        m,
        s,
        millis.abs()
    )
}

fn days_to_ymd(days: i64) -> (i32, i32, i32) {
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as i32;
    let mo = (mp + (if mp < 10 { 3 } else { -9 })) as i32;
    let y = y + (if mo <= 2 { 1 } else { 0 });
    (y, mo, d)
}

fn weekday_name(n: i64) -> &'static str {
    match n.rem_euclid(7) {
        0 => "Sun",
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        6 => "Sat",
        _ => "Sun",
    }
}

fn month_name(mo: i32) -> &'static str {
    match mo {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "Jan",
    }
}

pub fn to_string(args: &[Value], heap: &mut Heap) -> Value {
    let id = match args.first().and_then(Value::as_date_id) {
        Some(i) => i,
        None => return Value::String("Invalid Date".to_string()),
    };
    let ms = heap.date_timestamp(id);
    Value::String(format_date(ms))
}

pub fn to_iso_string(args: &[Value], heap: &mut Heap) -> Value {
    let id = match args.first().and_then(Value::as_date_id) {
        Some(i) => i,
        None => return Value::String("Invalid Date".to_string()),
    };
    let ms = heap.date_timestamp(id);
    if !ms.is_finite() {
        return Value::String("Invalid Date".to_string());
    }
    let secs = (ms / 1000.0) as i64;
    let millis = (ms % 1000.0) as i32;
    let t = secs % 86400;
    let h = t / 3600;
    let m = (t % 3600) / 60;
    let s = t % 60;
    let days = secs / 86400;
    let (y, mo, d) = days_to_ymd(days);
    Value::String(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        y,
        mo,
        d,
        h,
        m,
        s,
        millis.abs()
    ))
}

pub fn get_year(args: &[Value], heap: &mut Heap) -> Value {
    let id = match args.first().and_then(Value::as_date_id) {
        Some(i) => i,
        None => return Value::Number(f64::NAN),
    };
    let ms = heap.date_timestamp(id);
    if !ms.is_finite() {
        return Value::Number(f64::NAN);
    }
    let secs = (ms / 1000.0) as i64;
    let days = secs / 86400;
    let (y, _, _) = days_to_ymd(days);
    Value::Number((y - 1900) as f64)
}

pub fn get_full_year(args: &[Value], heap: &mut Heap) -> Value {
    let id = match args.first().and_then(Value::as_date_id) {
        Some(i) => i,
        None => return Value::Number(f64::NAN),
    };
    let ms = heap.date_timestamp(id);
    if !ms.is_finite() {
        return Value::Number(f64::NAN);
    }
    let secs = (ms / 1000.0) as i64;
    let days = secs / 86400;
    let (y, _, _) = days_to_ymd(days);
    Value::Number(y as f64)
}

pub fn set_year(args: &[Value], heap: &mut Heap) -> Value {
    let id = match args.first().and_then(Value::as_date_id) {
        Some(i) => i,
        None => return Value::Number(f64::NAN),
    };

    let date_value = heap.date_timestamp(id);
    let year_number = args.get(1).map(super::to_number).unwrap_or(f64::NAN);

    if year_number.is_nan() {
        heap.set_date_timestamp(id, f64::NAN);
        return Value::Number(f64::NAN);
    }
    if !year_number.is_finite() {
        heap.set_date_timestamp(id, f64::NAN);
        return Value::Number(f64::NAN);
    }

    let local_time = if date_value.is_nan() { 0.0 } else { date_value };
    let time_within_day = local_time.rem_euclid(86_400_000.0);
    let day_number = ((local_time - time_within_day) / 86_400_000.0) as i64;
    let (_, month, day) = days_to_ymd(day_number);

    let mut full_year = year_number.trunc() as i64;
    if (0..=99).contains(&full_year) {
        full_year += 1900;
    }
    if full_year < i32::MIN as i64 || full_year > i32::MAX as i64 {
        heap.set_date_timestamp(id, f64::NAN);
        return Value::Number(f64::NAN);
    }

    let new_day_number = ymd_to_days(full_year as i32, month, day);
    let new_time = new_day_number as f64 * 86_400_000.0 + time_within_day;
    let clipped = time_clip(new_time);
    heap.set_date_timestamp(id, clipped);
    Value::Number(clipped)
}

fn time_clip(time: f64) -> f64 {
    if !time.is_finite() || time.abs() > 8.64e15 {
        f64::NAN
    } else {
        time.trunc()
    }
}

fn ymd_to_days(y: i32, mo: i32, d: i32) -> i64 {
    let adj = if mo <= 2 { 1 } else { 0 };
    let y_adj = (y as i64).saturating_sub(adj as i64);
    let m = (mo + 9) % 12 + 1;
    let era = y_adj / 400;
    let yoe = y_adj % 400;
    let doy = (153 * (m as i64) + 2) / 5 + (d as i64) - 1;
    let doe = 365 * yoe + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn to_gmt_string(args: &[Value], heap: &mut Heap) -> Value {
    to_string(args, heap)
}
