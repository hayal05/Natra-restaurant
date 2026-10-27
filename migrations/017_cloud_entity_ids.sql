-- Stable mapping between cloud identities and local SQLite integer IDs.
-- A cloud ID is unique across devices; local row IDs are not.
CREATE TABLE IF NOT EXISTS cloud_entity_ids (
    entity_type TEXT NOT NULL CHECK (length(trim(entity_type)) > 0),
    entity_id TEXT NOT NULL CHECK (length(trim(entity_id)) > 0),
    local_id INTEGER NOT NULL CHECK (local_id > 0),
    PRIMARY KEY (entity_type, entity_id),
    UNIQUE (entity_type, local_id)
);
