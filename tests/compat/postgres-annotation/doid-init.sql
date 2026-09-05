CREATE TABLE tb_books (bk_title text NOT NULL);
INSERT INTO tb_books (bk_title)
SELECT 'NT MGI.' FROM generate_series(1, 76);
