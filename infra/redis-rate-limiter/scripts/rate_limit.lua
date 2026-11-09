local state = redis.call('HMGET', KEYS[1], 'tokens', 'last_fill', 'last_burst')
local tokens = tonumber(state[1])
local last_fill = tonumber(state[2])
local last_burst = tonumber(state[3])
local redis_time = redis.call('TIME')
local now = tonumber(redis_time[1])
local initial = tonumber(ARGV[1])
local fill_rate = tonumber(ARGV[2])
local burst_interval = tonumber(ARGV[3])
local max_cap = tonumber(ARGV[4])
local batch_size = tonumber(ARGV[5])
local ttl = tonumber(ARGV[6])

if not tokens then
    tokens = initial
    last_fill = now
    last_burst = now
else
    local elapsed_fill = now - last_fill
    if elapsed_fill > 0 then
        tokens = tokens + (elapsed_fill * fill_rate)
        last_fill = now
    end
    local elapsed_burst = now - last_burst
    if elapsed_burst >= burst_interval then
        local bursts = math.floor(elapsed_burst / burst_interval)
        tokens = tokens + (bursts * initial)
        last_burst = last_burst + (bursts * burst_interval)
    end
    if tokens > max_cap then
        tokens = max_cap
    end
end

local granted = 0
local retry_after = 0

if tokens > 0 then
    if tokens >= batch_size then
        granted = batch_size
        tokens = tokens - batch_size
    else
        granted = tokens
        tokens = 0
    end
else
    granted = 0
    if fill_rate > 0 then
        retry_after = 1
    else
        retry_after = burst_interval
    end
end

redis.call('HMSET', KEYS[1], 'tokens', tokens, 'last_fill', last_fill, 'last_burst', last_burst)
redis.call('EXPIRE', KEYS[1], ttl)

return { granted, retry_after }
