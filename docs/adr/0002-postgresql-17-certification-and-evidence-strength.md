# PostgreSQL 17 认证与对照断言强度

状态：accepted。当前 PostgreSQL 限定迁移只认证固定 digest 的 PostgreSQL 17 服务端，不暗示所有 PostgreSQL 主版本均受支持；新增版本须建立独立认证。每条对照情景按 Ontop 基线实际断言强度比较：基线提供 RDF/tuple/boolean/graph 结果时比较完整规范化结果，基线仅断言 `rsi:size` 时才可比较基数，且账本必须标明此限制。这样避免把单一服务器版本或弱断言误宣称为广泛功能等价。
