use google_drive3::api::{DriveHub, File, Scope};
use hyper_rustls::HttpsConnector;
use log::{debug, info};
use std::fs;
use std::io::Write;
use std::thread;
use std::time;
use yup_oauth2::{read_application_secret, InstalledFlowAuthenticator, InstalledFlowReturnMethod};

use crate::librote::error;

pub async fn upload_pdf(
    client_secret_file: &'static str,
    parent_id: &'static str,
    num_chunk: u8,
) -> Result<(), error::Error> {
    let items: Vec<u8> = (1..=num_chunk).collect();
    let tasks: Vec<_> = items
        .into_iter()
        .map(|i| {
            tokio::spawn(async move {
                let secret = read_application_secret(client_secret_file)
                    .await
                    .expect("Could not read secret from client_secret_file.json");
                let auth = InstalledFlowAuthenticator::builder(
                    secret,
                    InstalledFlowReturnMethod::HTTPRedirect,
                )
                .persist_tokens_to_disk("token.json")
                .build()
                .await
                .unwrap();
                let hub = DriveHub::new(
                    hyper::Client::builder().build(HttpsConnector::with_native_roots()),
                    auth,
                );
                info!("Uploading `chunk_{:02}.pdf`", i);
                let mut create_req = File::default();
                create_req.name = Some(format!("gd_chunk_{:02}", i));
                create_req.parents = Some(vec![parent_id.to_string()]);
                let create_result = hub
                    .files()
                    .create(create_req)
                    .use_content_as_indexable_text(true)
                    .supports_all_drives(true)
                    .ocr_language("ja")
                    .keep_revision_forever(true)
                    .ignore_default_visibility(true)
                    .enforce_single_parent(false)
                    .upload(
                        fs::File::open(format!("chunk_{:02}.pdf", i)).unwrap(),
                        "application/pdf".parse().unwrap(),
                    )
                    .await;
                let (_, pdf_file_resp) =
                    create_result.expect("Something went wrong when uploading pdf file");
                debug!("{:?}", pdf_file_resp);
                info!("Finished uploading `chunk_{:02}.pdf`", i);

                info!("OCR-ing `chunk_{:02}.pdf`", i);
                let pdf_file_id = pdf_file_resp
                    .id
                    .expect("pdf file_id does not exist in pdf_file_resp");
                let mut copy_req = File::default();
                copy_req.name = Some(format!("ocr_chunk_{:02}", i));
                copy_req.parents = Some(vec![parent_id.to_string()]);
                copy_req.mime_type = Some(String::from("application/vnd.google-apps.document"));
                let copy_result = hub
                    .files()
                    .copy(copy_req, &pdf_file_id)
                    .supports_all_drives(true)
                    .ocr_language("ja")
                    .keep_revision_forever(true)
                    .ignore_default_visibility(true)
                    .enforce_single_parent(false)
                    .doit()
                    .await;
                let (_, ocr_resp) = copy_result.expect("Something went wrong when OCR pdf file");
                debug!("{:?}", ocr_resp);
                info!("Finished OCR `chunk_{:02}.pdf`", i);

                info!("Downloading OCR result of `chunk_{:02}.pdf`", i);
                let ocr_file_id = ocr_resp
                    .id
                    .expect("gdocs ocr file_id does not exist in ocr_resp");
                let mut export_req = File::default();
                export_req.parents = Some(vec![parent_id.to_string()]);
                info!("Finished downloading OCR result of `chunk_{:02}.pdf`", i);

                let export_result = hub
                    .files()
                    .export(&ocr_file_id, "text/html")
                    .param("alt", "media")
                    // technically don't need full, but if we use default File
                    // then we will have reauth to grant this permission
                    .add_scope(Scope::Full)
                    .doit()
                    .await
                    .expect("could not export ocr'd file");
                let mut ostream = fs::OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(format!("ocr_{:02}.html", i))
                    .expect("could not create outstream to write html result");
                let bytes = hyper::body::to_bytes(export_result.into_body())
                    .await
                    .expect("a string as API currently is inefficient")
                    .to_vec();
                ostream.write_all(&bytes).expect("write to be complete");
                ostream
                    .flush()
                    .expect("io to never fail which should really be fixed one day");
            })
        })
        .collect();

    for task in tasks {
        task.await
            .expect("could not execute one of the upload/ocr task");
        thread::sleep(time::Duration::from_millis(1000));
    }
    Ok(())
}
