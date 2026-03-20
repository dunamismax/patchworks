-- @fixture schema_left
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT,
    updated_at TEXT DEFAULT '2026-01-01T00:00:00Z'
);

CREATE TABLE audit_log (
    id INTEGER PRIMARY KEY,
    action TEXT NOT NULL
);

INSERT INTO users (id, name, email) VALUES
    (1, 'Ada', 'ada@example.com'),
    (2, 'Linus', NULL);

INSERT INTO audit_log (id, action) VALUES
    (1, 'seed');

-- @fixture schema_right
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    updated_at TEXT DEFAULT '2026-02-01T00:00:00Z',
    status TEXT DEFAULT 'active'
);

CREATE TABLE release_notes (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL
);

INSERT INTO users (id, name, email, status) VALUES
    (1, 'Ada', 'ada@example.com', 'active'),
    (3, 'Grace', 'grace@example.com', 'active');

INSERT INTO release_notes (id, title) VALUES
    (1, 'v1');

-- @fixture data_left
CREATE TABLE items (
    id INTEGER PRIMARY KEY,
    label TEXT,
    quantity INTEGER,
    price REAL,
    payload BLOB
);

INSERT INTO items (id, label, quantity, price, payload) VALUES
    (1, 'alpha', 10, 1.50, X'AA'),
    (2, 'beta', 4, 2.25, X'BB'),
    (3, NULL, 8, 3.00, NULL);

-- @fixture data_right
CREATE TABLE items (
    id INTEGER PRIMARY KEY,
    label TEXT,
    quantity INTEGER,
    price REAL,
    payload BLOB
);

INSERT INTO items (id, label, quantity, price, payload) VALUES
    (1, 'alpha', 10, 1.50, X'AA'),
    (2, 'beta-updated', 7, 2.25, X'BC'),
    (4, NULL, 1, 4.75, NULL);

-- @fixture rowid_left
CREATE TABLE notes (
    body TEXT,
    archived INTEGER
);

INSERT INTO notes (body, archived) VALUES
    ('draft', 0),
    ('publish', 1);

-- @fixture rowid_right
CREATE TABLE notes (
    body TEXT,
    archived INTEGER
);

INSERT INTO notes (body, archived) VALUES
    ('draft updated', 0),
    ('publish', 1),
    ('new note', 0);

-- @fixture snapshot_source
CREATE TABLE projects (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

INSERT INTO projects (id, name) VALUES
    (1, 'Patchworks'),
    (2, 'Workbench');
