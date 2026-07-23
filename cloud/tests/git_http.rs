use cowiki_cloud::git_http::{
    GitHttpRequest, GitService, authorize_service, classify_request, parse_cgi_response,
    run_git_http_backend,
};
use cowiki_cloud::git_repo::GitRepoStore;
use cowiki_cloud::model::MemberRole;
use http::{HeaderMap, Method, StatusCode};
use uuid::Uuid;

#[test]
fn smart_http_routes_classify_only_expected_services() {
    assert_eq!(
        classify_request(&Method::GET, "info/refs", Some("service=git-upload-pack")).unwrap(),
        GitService::UploadPack
    );
    assert_eq!(
        classify_request(&Method::POST, "git-receive-pack", None).unwrap(),
        GitService::ReceivePack
    );
    assert!(classify_request(&Method::GET, "../../etc/passwd", None).is_err());
    assert!(classify_request(&Method::POST, "git-upload-archive", None).is_err());
}

#[test]
fn viewers_fetch_but_only_editors_and_above_push() {
    assert!(authorize_service(MemberRole::Viewer, GitService::UploadPack).is_ok());
    assert!(authorize_service(MemberRole::Viewer, GitService::ReceivePack).is_err());
    assert!(authorize_service(MemberRole::Editor, GitService::ReceivePack).is_ok());
    assert!(authorize_service(MemberRole::Manager, GitService::ReceivePack).is_ok());
    assert!(authorize_service(MemberRole::Owner, GitService::ReceivePack).is_ok());
}

#[test]
fn cgi_headers_and_status_are_parsed_without_forwarding_status_header() {
    let response = parse_cgi_response(
        b"Status: 401 Unauthorized\r\nContent-Type: text/plain\r\nWWW-Authenticate: Bearer\r\n\r\nnope"
            .to_vec(),
    )
    .unwrap();
    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers["content-type"], "text/plain");
    assert_eq!(response.headers["www-authenticate"], "Bearer");
    assert_eq!(response.body, b"nope");
    assert!(response.headers.get("status").is_none());
}

#[tokio::test]
async fn real_git_http_backend_advertises_a_bare_space() {
    let root = tempfile::tempdir().unwrap();
    let store = GitRepoStore::new(root.path()).unwrap();
    let space = Uuid::new_v4();
    let user = Uuid::new_v4();
    store.ensure_space(space).unwrap();

    let response = run_git_http_backend(
        &store,
        space,
        user,
        MemberRole::Viewer,
        GitHttpRequest {
            method: Method::GET,
            path: "info/refs".into(),
            query: Some("service=git-upload-pack".into()),
            headers: HeaderMap::new(),
            body: Vec::new(),
            bootstrap: false,
        },
    )
    .await
    .unwrap();

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        response.headers["content-type"],
        "application/x-git-upload-pack-advertisement"
    );
    assert!(
        response
            .body
            .windows(15)
            .any(|part| part == b"git-upload-pack")
    );
}
