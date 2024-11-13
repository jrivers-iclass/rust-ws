#[derive(Debug)]
pub struct AuthError;
impl warp::reject::Reject for AuthError {}