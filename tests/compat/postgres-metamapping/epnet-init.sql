CREATE TABLE relativeposition (
  id integer PRIMARY KEY
);
CREATE TABLE inscription (
  id integer PRIMARY KEY,
  relativeposition integer NOT NULL REFERENCES relativeposition(id)
);
INSERT INTO relativeposition (id) VALUES (4), (5);
INSERT INTO inscription (id, relativeposition) VALUES (101, 4), (102, 5);
