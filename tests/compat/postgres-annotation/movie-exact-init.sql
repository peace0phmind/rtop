-- 本 fixture 需要 790 个 NULL year，以保留基线 annotation 的 description/date 差异；
-- 前序场景可将同名 title 列建为 NOT NULL，故在此明确恢复本场景 schema 前置条件。
ALTER TABLE title ALTER COLUMN production_year DROP NOT NULL;
TRUNCATE title, movie_info, cast_info, movie_companies;
INSERT INTO title (id, title, production_year, kind_id)
SELECT value, 'title-' || value,
       CASE WHEN value <= 443300 THEN 2020 ELSE NULL END,
       1
FROM generate_series(1, 444090) AS value;
INSERT INTO movie_info (movie_id, info, info_type_id)
SELECT value, 'genre', 3 FROM generate_series(1, 546032) AS value;
INSERT INTO movie_info (movie_id, info, info_type_id)
SELECT value, 'gross', 107 FROM generate_series(1, 112576) AS value;
INSERT INTO cast_info (person_id, movie_id, role_id)
SELECT value, value, 1 FROM generate_series(1, 100000) AS value;
INSERT INTO movie_companies (company_id, company_type_id)
SELECT value, 2 FROM generate_series(1, 131645) AS value;
