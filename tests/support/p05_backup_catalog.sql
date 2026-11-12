SELECT jsonb_build_object(
 'columns',(SELECT jsonb_agg(jsonb_build_array(table_name,column_name,data_type,udt_name,is_nullable,column_default,is_identity,identity_generation,is_generated,generation_expression,collation_name,character_maximum_length,numeric_precision,numeric_scale,datetime_precision) ORDER BY table_name,ordinal_position) FROM information_schema.columns WHERE table_schema='public'),
 'constraints',(SELECT jsonb_agg(jsonb_build_array(c.relname,k.conname,pg_get_constraintdef(k.oid,true)) ORDER BY c.relname,k.conname) FROM pg_constraint k JOIN pg_class c ON c.oid=k.conrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public'),
 'indexes',(SELECT jsonb_agg(jsonb_build_array(indexname,indexdef) ORDER BY indexname) FROM pg_indexes WHERE schemaname='public'),
 'relations',(SELECT jsonb_agg(jsonb_build_array(c.relname,c.relkind::text,c.relpersistence::text,c.relrowsecurity,c.relforcerowsecurity) ORDER BY c.relname) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND c.relkind IN ('r','p','v','m','f','S')),
 'sequences',(SELECT jsonb_agg(jsonb_build_array(sequencename,data_type::text,start_value,min_value,max_value,increment_by,cycle,cache_size) ORDER BY sequencename) FROM pg_sequences WHERE schemaname='public'),
 'types',(SELECT jsonb_agg(jsonb_build_array(t.typname,t.typtype::text,t.typelem::regtype::text,t.typrelid::regclass::text,t.typnotnull,t.typdefault) ORDER BY t.typname) FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace WHERE n.nspname='public')
)
