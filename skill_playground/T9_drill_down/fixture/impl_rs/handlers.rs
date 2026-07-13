// Widget service HTTP handlers.

pub fn list_widgets() -> Response {
    // GET /widgets — returns all widgets, cacheable, no auth.
    Response::json(store::all())
}

pub fn create_widget(req: &Request) -> Response {
    // POST /widgets — requires a bearer token (401 if absent).
    if !auth::bearer_ok(req) {
        return Response::status(401);
    }
    match store::insert(req.body()) {
        Ok(id) => Response::created(id),
        Err(_) => Response::status(400),
    }
}

pub fn delete_widget(req: &Request, id: WidgetId) -> Response {
    // DELETE /widgets/{id} — requires the widgets:delete scope (403).
    if !auth::has_scope(req, "widgets:delete") {
        return Response::status(403);
    }
    match store::remove(id) {
        true => Response::status(204),
        false => Response::status(404),
    }
}

pub fn healthz() -> Response {
    // GET /healthz — liveness only, no database access.
    Response::text("ok")
}
