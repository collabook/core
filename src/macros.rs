#[macro_export]
macro_rules! badrequest {
    ($expr:expr) => {
        match $expr {
            Ok(val) => val,
            Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
        }
    };
}

#[macro_export]
macro_rules! none {
    ($expr:expr, $msg:expr) => {
        match $expr {
            Some(val) => val,
            None => return HttpResponse::BadRequest().body($msg),
        }
    };
}

#[macro_export]
macro_rules! git2_error {
    ($expr:expr) => {
        match $expr {
            Ok(val) => val,
            Err(e) => return Err(git2::Error::from_str(&e.to_string())),
        }
    };
}
