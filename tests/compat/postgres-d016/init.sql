CREATE TABLE patient_types (
  id integer PRIMARY KEY,
  birthdate date NOT NULL,
  entrancedate timestamp NOT NULL,
  paid boolean NOT NULL
);
INSERT INTO patient_types VALUES (10, '1981-10-10', '2009-10-10 12:12:22', false);
