-- 可重放的 LUBM 最小 PostgreSQL fixture。行数来自固定 manifest 的 rsi:size，
-- 但 schema/关系只使用固定 lubm-pgsql.obda 的 source relation 和 column。
CREATE TABLE students (
  depid integer, uniid integer, id integer, stype integer, name text,
  degreeuniid integer, email text, phone text, advisorid integer, advisortype integer
);
CREATE TABLE takescourses (
  depid integer, uniid integer, studtype integer, studid integer, coursetype integer, courseid integer
);
CREATE TABLE teachers (
  depid integer, uniid integer, ttype integer, id integer, name text,
  underD integer, masterD integer, docD integer, email text, phone text, research text
);
CREATE TABLE publications (depid integer, uniid integer, publicationid integer, authortype integer, authorid integer);
CREATE TABLE courses (depid integer, uniid integer, ctype integer, id integer, teacherid integer, teachertype integer);
CREATE TABLE researchgroups (depid integer, uniid integer, id integer);
CREATE TABLE departments (departmentid integer, universityid integer);
CREATE TABLE heads (depid integer, uniid integer, profid integer);

-- 5,916 undergraduate + 1,874 graduate：OWL subclass closure 后 Student=7,790。
INSERT INTO students
SELECT CASE WHEN id <= 715 THEN 0 ELSE 1 END, 0, id, 0,
       'undergraduate-' || id, 1, 'undergraduate-' || id || '@example.test', '100-' || id,
       CASE WHEN id <= 208 THEN 1 ELSE 0 END,
       CASE WHEN id <= 208 THEN 1 WHEN id BETWEEN 209 AND 275 THEN 0 ELSE 2 END
FROM generate_series(0, 5915) AS id;
INSERT INTO students
SELECT CASE WHEN id <= 3 THEN 0 ELSE 1 END, 0, id, 1, 'graduate-' || id, 1, 'graduate-' || id || '@example.test', '200-' || id, 0, 2
FROM generate_series(0, 1873) AS id;

-- query 1/10 的四个 graduate-course answer；query 7 的 67 与 query 9 的 208
-- 分别使用不同 faculty/course，以免选择性互相污染。
INSERT INTO takescourses SELECT 0, 0, 1, id, 1, 0 FROM generate_series(0, 3) AS id;
INSERT INTO takescourses SELECT 0, 0, 0, id, 0, 1 FROM generate_series(209, 275) AS id;
INSERT INTO takescourses SELECT 0, 0, 0, id, 0, 2 FROM generate_series(1, 208) AS id;
INSERT INTO teachers
SELECT 0, 0, 1, id, 'assistant-' || id, CASE WHEN id = 0 THEN 0 ELSE 1 END, 1, 1,
       'assistant-' || id || '@example.test', '300-' || id, 'research'
FROM generate_series(0, 33) AS id;
INSERT INTO teachers
SELECT 2, 0, 3, id, 'full-' || id, 1, 1, 1, 'full-' || id || '@example.test', '400-' || id, 'research'
FROM generate_series(0, 14) AS id;
INSERT INTO courses VALUES (0, 0, 0, 1, 0, 2), (0, 0, 0, 2, 1, 1);
INSERT INTO publications SELECT 0, 0, id, 1, 0 FROM generate_series(0, 5) AS id;
INSERT INTO researchgroups SELECT 1, 0, id FROM generate_series(0, 223) AS id;
INSERT INTO departments VALUES (0, 0), (1, 0), (2, 0);
INSERT INTO heads SELECT 2, 0, id FROM generate_series(0, 14) AS id;
