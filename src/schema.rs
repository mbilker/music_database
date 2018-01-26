table! {
    acoustid_last_check (id) {
        id -> Int4,
        library_id -> Nullable<Int4>,
        last_check -> Nullable<Timestamptz>,
    }
}

table! {
    library (id) {
        id -> Int4,
        mbid -> Nullable<Uuid>,
        title -> Nullable<Varchar>,
        artist -> Nullable<Varchar>,
        album -> Nullable<Varchar>,
        track -> Nullable<Varchar>,
        track_number -> Oid,
        duration -> Oid,
        path -> Nullable<Varchar>,
    }
}

joinable!(acoustid_last_check -> library (library_id));

allow_tables_to_appear_in_same_query!(
    acoustid_last_check,
    library,
);
