use base64::{engine::general_purpose, Engine as _};
use metaflac::Tag;

#[derive(Default, Clone)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub cover: String,
}

pub fn extract_metadata(path: &str) -> TrackMetadata {
    let tag = Tag::read_from_path(path).unwrap();
    let mut metadata = TrackMetadata {
        ..Default::default()
    };

    if let Some(vorbis) = tag.vorbis_comments() {
        metadata.title = vorbis
            .title()
            .unwrap_or(&vec![path.split('/').last().unwrap().to_string()])
            .join(" ");
        metadata.artist = vorbis
            .artist()
            .unwrap_or(&vec!["Unknown Artist".into()])
            .join(" ");
        metadata.album = vorbis
            .album()
            .unwrap_or(&vec!["Unknown Album".into()])
            .join(" ");
    }

    if let Some(picture) = tag.pictures().next() {
        let data = picture.data.clone();
        metadata.cover = format!(
            "data:{};base64,{}",
            picture.mime_type,
            general_purpose::STANDARD.encode(data)
        );
    } else {
        metadata.cover = "data:image/svg+xml;base64,PD94bWwgdmVyc2lvbj0iMS4wIiBlbmNvZGluZz0iaXNvLTg4NTktMSI/Pg0KPCEtLSBVcGxvYWRlZCB0bzogU1ZHIFJlcG8sIHd3dy5zdmdyZXBvLmNvbSwgR2VuZXJhdG9yOiBTVkcgUmVwbyBNaXhlciBUb29scyAtLT4NCjxzdmcgZmlsbD0iIzAwMDAwMCIgaGVpZ2h0PSI4MDBweCIgd2lkdGg9IjgwMHB4IiB2ZXJzaW9uPSIxLjEiIGlkPSJDYXBhXzEiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgeG1sbnM6eGxpbms9Imh0dHA6Ly93d3cudzMub3JnLzE5OTkveGxpbmsiIA0KCSB2aWV3Qm94PSIwIDAgNDc3LjIxNiA0NzcuMjE2IiB4bWw6c3BhY2U9InByZXNlcnZlIj4NCjxnPg0KCTxwYXRoIGQ9Ik00NTMuODU4LDEwNS4xMTZ2LTkxLjZjMC00LjMtMi4xLTguNC01LjUtMTAuOWMtMy41LTIuNS04LTMuMy0xMi4xLTJsLTI3Mi45LDg2LjdjLTUuNiwxLjgtOS40LDctOS40LDEyLjl2OTEuN3YwLjF2MTc1LjMNCgkJYy0xNC4zLTkuOS0zMi42LTE1LjMtNTEuOC0xNS4zYy0yMC4zLDAtMzkuNiw2LjEtNTQuMywxNy4xYy0xNS44LDExLjktMjQuNSwyOC0yNC41LDQ1LjVzOC43LDMzLjYsMjQuNSw0NS41DQoJCWMxNC43LDExLDMzLjksMTcuMSw1NC4zLDE3LjFzMzkuNi02LjEsNTQuMy0xNy4xYzE1LjgtMTEuOSwyNC41LTI4LDI0LjUtNDUuNXYtMjEyLjhsMjQ1LjktNzguMnYxNTYuNg0KCQljLTE0LjMtOS45LTMyLjYtMTUuMy01MS44LTE1LjNjLTIwLjMsMC0zOS42LDYuMS01NC4zLDE3LjFjLTE1LjgsMTEuOS0yNC41LDI4LTI0LjUsNDUuNXM4LjcsMzMuNiwyNC41LDQ1LjUNCgkJYzE0LjcsMTEsMzMuOSwxNy4xLDU0LjMsMTcuMXMzOS42LTYuMSw1NC4zLTE3LjFjMTUuOC0xMS45LDI0LjUtMjgsMjQuNS00NS41di0yMjIuMw0KCQlDNDUzLjg1OCwxMDUuMTE2LDQ1My44NTgsMTA1LjExNiw0NTMuODU4LDEwNS4xMTZ6IE0xMDIuMTU4LDQ1MC4yMTZjLTI4LjEsMC01MS44LTE2LjMtNTEuOC0zNS42YzAtMTkuMywyMy43LTM1LjYsNTEuOC0zNS42DQoJCXM1MS44LDE2LjMsNTEuOCwzNS42QzE1My45NTgsNDM0LjAxNiwxMzAuMjU4LDQ1MC4yMTYsMTAyLjE1OCw0NTAuMjE2eiBNMTgwLjk1OCwxNzMuNDE2di02My40bDI0NS45LTc4LjF2NjMuNEwxODAuOTU4LDE3My40MTZ6DQoJCSBNMzc1LjE1OCwzNjMuMTE2Yy0yOC4xLDAtNTEuOC0xNi4zLTUxLjgtMzUuNmMwLTE5LjMsMjMuNy0zNS42LDUxLjgtMzUuNnM1MS44LDE2LjMsNTEuOCwzNS42DQoJCUM0MjYuODU4LDM0Ni44MTYsNDAzLjE1OCwzNjMuMTE2LDM3NS4xNTgsMzYzLjExNnoiLz4NCjwvZz4NCjwvc3ZnPg==".into();
    }

    metadata
}
