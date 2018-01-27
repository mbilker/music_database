table! {
    acoustid_last_checks (id) {
        id -> Int4,
        library_id -> Int4,
        last_check -> Timestamptz,
    }
}

table! {
    library (id) {
        id -> Int4,
        path -> Varchar,
        title -> Nullable<Varchar>,
        artist -> Nullable<Varchar>,
        album -> Nullable<Varchar>,
        track -> Nullable<Varchar>,
        track_number -> Oid,
        duration -> Oid,
        mbid -> Nullable<Uuid>,
    }
}

joinable!(acoustid_last_checks -> library (library_id));

allow_tables_to_appear_in_same_query!(
    acoustid_last_checks,
    library,
);
