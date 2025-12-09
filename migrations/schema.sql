-- Create a table for public profiles
create table profiles (
  id uuid references auth.users not null primary key,
  email text,
  full_name text,
  avatar_url text,
  updated_at timestamp with time zone,
  
  constraint username_length check (char_length(full_name) >= 3)
);

-- Set up Row Level Security (RLS)
alter table profiles enable row level security;

create policy "Public profiles are viewable by everyone."
  on profiles for select
  using ( true );

create policy "Users can insert their own profile."
  on profiles for insert
  with check ( auth.uid() = id );

create policy "Users can update own profile."
  on profiles for update
  using ( auth.uid() = id );

-- Create a table for subscriptions
-- NOTE: Tier names must match Rust pricing.rs: free, starter, pro, enterprise
create type subscription_tier as enum ('free', 'starter', 'pro', 'enterprise');

create table subscriptions (
  id uuid default gen_random_uuid() primary key,
  user_id uuid references auth.users not null unique,
  tier subscription_tier default 'free',
  status text check (status in ('active', 'canceled', 'past_due')) default 'active',
  current_period_start timestamp with time zone default now(),
  current_period_end timestamp with time zone,
  created_at timestamp with time zone default now(),
  updated_at timestamp with time zone default now()
);

alter table subscriptions enable row level security;

create policy "Users can view own subscription."
  on subscriptions for select
  using ( auth.uid() = user_id );

-- Create a table for usage logs (GPU hours, etc.)
create table usage_logs (
  id uuid default gen_random_uuid() primary key,
  user_id uuid references auth.users not null,
  resource_type text not null, -- 'gpu_vram', 'compute_time'
  amount numeric not null,
  recorded_at timestamp with time zone default now()
);

alter table usage_logs enable row level security;

create policy "Users can view own usage logs."
  on usage_logs for select
  using ( auth.uid() = user_id );

-- Function to handle new user signup
create or replace function public.handle_new_user()
returns trigger as $$
begin
  insert into public.profiles (id, email, full_name, avatar_url)
  values (new.id, new.email, new.raw_user_meta_data->>'full_name', new.raw_user_meta_data->>'avatar_url');
  
  insert into public.subscriptions (user_id)
  values (new.id);
  
  return new;
end;
$$ language plpgsql security definer;

-- Trigger the function every time a user is created
create trigger on_auth_user_created
  after insert on auth.users
  for each row execute procedure public.handle_new_user();
