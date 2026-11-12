CREATE EXTENSION IF NOT EXISTS vector;
CREATE TABLE memory_vector_sets (
 individual_id uuid NOT NULL,
 memory_id uuid NOT NULL,
 profile text NOT NULL,
 memory_version bigint NOT NULL,
 privacy_epoch bigint NOT NULL,
 content_digest text NOT NULL,
 chunk_count integer NOT NULL CHECK(chunk_count BETWEEN 1 AND 128),
 PRIMARY KEY(individual_id,memory_id,profile),
 FOREIGN KEY(individual_id,memory_id) REFERENCES memories(individual_id,id)
);
CREATE TABLE memory_vectors (
 individual_id uuid NOT NULL,
 memory_id uuid NOT NULL,
 profile text NOT NULL,
 ordinal integer NOT NULL CHECK(ordinal>=0),
 start_char integer NOT NULL CHECK(start_char>=0),
 end_char integer NOT NULL CHECK(end_char>start_char),
 embedding vector(384) NOT NULL CHECK(vector_norm(embedding) BETWEEN 0.999 AND 1.001),
 PRIMARY KEY(individual_id,memory_id,profile,ordinal),
 FOREIGN KEY(individual_id,memory_id,profile) REFERENCES memory_vector_sets(individual_id,memory_id,profile) ON DELETE CASCADE
);
-- Exact search, deliberately no HNSW/IVFFlat index in the first version.
