use crate::book::{Book, CompileBookRequest};
use crate::error;
use actix::{Actor, Addr, Handler, Message, SyncContext};
use std::fs;

pub struct AppState {
    pub compiler: Addr<BookCompiler>,
}

pub struct BookCompiler {
    pub pdf_app: wkhtmltopdf::PdfApplication,
}

impl Actor for BookCompiler {
    type Context = SyncContext<Self>;
}

impl Message for CompileBookRequest {
    type Result = Result<(), error::MyError>;
}

impl Handler<CompileBookRequest> for BookCompiler {
    type Result = Result<(), error::MyError>;

    fn handle(&mut self, msg: CompileBookRequest, _: &mut Self::Context) -> Self::Result {
        let book = Book::open(msg.location.as_ref())?;
        let content = book.combine_content(msg.ids.as_slice())?;

        let mut pdfout = self
            .pdf_app
            .builder()
            .orientation(wkhtmltopdf::Orientation::Landscape)
            .margin(wkhtmltopdf::Size::Millimeters(10))
            .title("Book")
            .build_from_html(&content)?;

        fs::create_dir_all(msg.location.join("target"))?;
        let path = msg.location.join("target/book.pdf");
        pdfout.save(path)?;
        Ok(())
    }
}

/*
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use actix::{System, SyncArbiter};

    #[test]
    fn compile_book_works() {
        let pdf_app = wkhtmltopdf::PdfApplication::new().unwrap();
        let location = PathBuf::from("test");
        let ids = vec!["1".to_string()];
        let msg = CompileBookRequest { location, ids };
        assert!(true, true);

        System::run(move || {
            let addr = SyncArbiter::start(1, move || BookCompiler {
                pdf_app
            });
            addr.do_send(msg);
        });
    }
}
*/
