-- BindWithFunctionsPostgreSQLTest 的可观察 STR/TZ instant：输入列按 UTC
-- 解释，adapter 以 Europe/Rome 呈现，覆盖 CET 与 CEST。
UPDATE books SET publication_date = CASE id
  WHEN 1 THEN TIMESTAMP '2014-07-14 10:47:52'
  WHEN 2 THEN TIMESTAMP '2011-12-08 11:30:00'
  WHEN 3 THEN TIMESTAMP '2015-09-21 09:23:06'
  WHEN 4 THEN TIMESTAMP '1967-11-05 06:50:00'
END;
