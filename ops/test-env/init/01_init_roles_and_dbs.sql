-- Dennett D0 isolated test PostgreSQL initialization script.
-- Creates two least-privilege roles and two strictly separated *_test databases.
-- Safe to run via /docker-entrypoint-initdb.d/ or `psql -v ON_ERROR_STOP=1 -f`.

CREATE ROLE dennett_control_role
    NOSUPERUSER
    NOCREATEDB
    NOCREATEROLE
    NOINHERIT
    NOREPLICATION
    NOBYPASSRLS
    LOGIN
    PASSWORD 'dennett_control_test_pw';

CREATE ROLE dennett_memory_role
    NOSUPERUSER
    NOCREATEDB
    NOCREATEROLE
    NOINHERIT
    NOREPLICATION
    NOBYPASSRLS
    LOGIN
    PASSWORD 'dennett_memory_test_pw';

CREATE DATABASE dennett_control_test OWNER dennett_control_role;
CREATE DATABASE dennett_memory_test OWNER dennett_memory_role;

REVOKE ALL ON DATABASE postgres FROM PUBLIC;
REVOKE ALL ON DATABASE template1 FROM PUBLIC;
REVOKE ALL ON DATABASE dennett_control_test FROM PUBLIC;
REVOKE ALL ON DATABASE dennett_memory_test FROM PUBLIC;

REVOKE CONNECT ON DATABASE dennett_control_test FROM PUBLIC;
REVOKE CONNECT ON DATABASE dennett_memory_test FROM PUBLIC;
REVOKE CONNECT ON DATABASE dennett_control_test FROM dennett_memory_role;
REVOKE CONNECT ON DATABASE dennett_memory_test FROM dennett_control_role;

GRANT CONNECT, CREATE, TEMPORARY ON DATABASE dennett_control_test TO dennett_control_role;
GRANT CONNECT, CREATE, TEMPORARY ON DATABASE dennett_memory_test TO dennett_memory_role;
