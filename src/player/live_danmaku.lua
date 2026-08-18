-- Render live Bilibili danmaku in one persistent MPV OSD layer.

local mp = require "mp"
local utils = require "mp.utils"

local MAX_ACTIVE = 320
local MAX_PENDING = 512
local config = {
    enabled = true, display_area = 0.5, opacity = 1.0, font_scale = 1.0,
    duration = 7.0, stroke_width = 2.0, line_height = 1.6,
    massive_mode = false, font_family = "sans-serif",
    advanced_offset_x = 0.0, advanced_offset_y = 0.0, advanced_scale = 1.0,
}

local active, pending, lane_ready = {}, {}, {}
local next_lane = 1
local overlay = mp.create_osd_overlay("ass-events")
overlay.z = 20
local refresh_timer = nil
local fps_probe_timer = nil

-- Pause-aware clock so danmaku freeze while the video is paused (the wall
-- clock keeps running, which would otherwise keep scrolling comments moving).
-- "now" used by the renderer (and lane scheduling) is this monotonic value.
local clock_base = mp.get_time()
local clock_offset = 0.0        -- accumulated paused duration
local paused = false

-- Total elapsed *unpaused* time. While playing this is the accumulator
-- plus the current unpaused segment; while paused it is just the accumulator
-- (frozen), so danmaku ages never advance during a pause.
local function now_clock()
    if paused then
        return clock_offset
    end
    return clock_offset + (mp.get_time() - clock_base)
end

local function clamp(value, low, high)
    return math.max(low, math.min(high, value))
end

local function utf8_count(text)
    local _, count = text:gsub("[^\128-\191]", "")
    return count
end

local function escape_ass(text)
    return text:gsub("\\", "\\\\"):gsub("{", "\\{"):gsub("}", "\\}"):gsub("[\r\n]", " ")
end

-- BAS payloads mark line breaks with `/n`. Convert them to ASS `\N` only
-- after escaping, otherwise the backslash gets doubled into a literal `\N`
-- printed on screen.
local function to_ass_text(text)
    return escape_ass(text):gsub("/n", "\\N")
end

local function safe_font_name(text)
    return tostring(text or "sans-serif"):gsub("[\\{}]", "")
end

-- Legacy payloads carry font names like `"SimHei, \"Microsoft JhengHei\""`
-- or `"\"Microsoft YaHei\""` with stray backslashes and quotes. A raw
-- comma/quote/backslash inside \fn can confuse the ASS parser, so take the
-- first family only and strip surrounding quotes and backslashes.
local function clean_font_name(font)
    if type(font) ~= "string" or font == "" then return nil end
    local cleaned = tostring(font):gsub("[\\\"]", "")
    local first = cleaned:match("^%s*([^,]+)")
    if first ~= nil then
        first = first:gsub("%s+$", "")
        if first ~= "" then return first end
    end
    return cleaned:gsub("%s+$", "")
end

local function ass_color(value)
    local color = clamp(tonumber(value) or 0xFFFFFF, 0, 0xFFFFFF)
    local red = math.floor(color / 65536) % 256
    local green = math.floor(color / 256) % 256
    local blue = color % 256
    return string.format("%02X%02X%02X", blue, green, red)
end

local function layout(width, height)
    local font_size = math.max(18, math.floor(height / 30 * config.font_scale))
    local lane_height = math.max(font_size + 4, math.floor(font_size * config.line_height))
    local top = math.floor(height * 0.04)
    local usable = math.max(lane_height, math.floor(height * config.display_area) - top)
    return font_size, lane_height, top, math.max(1, math.floor(usable / lane_height))
end

local function select_lane(now, lanes)
    for lane = 1, lanes do
        if (lane_ready[lane] or 0) <= now then return lane end
    end
    if config.massive_mode then
        local lane = ((next_lane - 1) % lanes) + 1
        next_lane = lane % lanes + 1
        return lane
    end
    return nil
end

-- Cap how many pending comments are scheduled per frame. In a dense section
-- the queue can hold hundreds of comments; pulling them all into the active
-- list in one frame turns a single OSD rebuild into a giant one. A small
-- budget keeps per-frame work flat and lets the lane allocator spread the
-- load across frames (the video clock gate in render() keeps timing exact).
local SCHEDULE_BUDGET = 24

local function schedule(width, height, now)
    local font_size, lane_height, top, lanes = layout(width, height)
    local pixels_per_second = width / config.duration
    local safety_gap = math.max(40, font_size * 2)
    local pos = mp.get_property_number("time-pos", 0) or 0
    local budget = SCHEDULE_BUDGET
    while #pending > 0 and #active < MAX_ACTIVE and budget > 0 do
        budget = budget - 1
        -- Pending is FIFO in video-time order (the Rust sender walks the
        -- sorted danmaku list), so only the head needs checking.
        local head = pending[1]
        if head.video_time and head.video_time > pos + 0.5 then
            -- Not due yet (early arrival after a seek); wait for the clock.
            break
        end
        if head.video_time and head.video_time < pos - 0.5 then
            -- Expired while queued (backward seek replay); drop it.
            table.remove(pending, 1)
        else
            local lane = select_lane(now, lanes)
            if lane == nil then break end
            local message = table.remove(pending, 1)
            local characters = math.max(1, utf8_count(message.text))
            -- CJK glyphs are approximately one em wide. The former 0.62-em
            -- estimate released a lane while bold Chinese text was still visible,
            -- allowing the following comment to catch and overlap it.
            local text_width = math.max(font_size * 2, characters * font_size * 1.05)
            message.created = now
            message.lane = lane
            message.text_width = text_width
            message.duration = (width + text_width + safety_gap) / pixels_per_second
            lane_ready[lane] = now + (text_width + safety_gap) / pixels_per_second
            active[#active + 1] = message
        end
    end
    return font_size, lane_height, top, lanes
end


-- Rebuilding the whole ASS overlay costs real CPU, and on a 120/144 Hz
-- display the old code ran the rebuild at the panel rate even when only a
-- handful of comments were on screen. Cap the render loop at 60 fps and
-- drop the cadence as the screen fills up: 60 fps below 80 comments,
-- 45 fps below 160, 30 fps beyond that. Danmaku animation does not need
-- panel-rate updates, and the frame budget is where the stutter comes from.
local function target_fps(active_count)
    if active_count >= 160 then return 30 end
    if active_count >= 80 then return 45 end
    return 60
end

local current_fps = 60

local function render()
    local width, height = mp.get_osd_size()
    if width <= 0 or height <= 0 then return end
    if not config.enabled then
        overlay.data = ""
        overlay:update()
        return
    end
    if #active == 0 and #pending == 0 and overlay.data == "" then
        return
    end

    local now = now_clock()
    local font_size, lane_height, top, lanes = schedule(width, height, now)
    local alpha = math.floor((1.0 - config.opacity) * 255 + 0.5)
    local lines, remaining = {}, {}
    local pos = mp.get_property_number("time-pos", 0) or 0
    -- Render the rolling/top/bottom danmaku first, then the positioned
    -- (BAS) ones on top. ASS draws later lines over earlier ones, so this
    -- keeps the advanced lyric animation above the scrolling crowd instead
    -- of letting a mode-1 comment paint over it at the same spot.
    for _, message in ipairs(active) do
        if not message.positioned then
            local age = now - message.created
            if age < message.duration then
                local progress = clamp(age / message.duration, 0, 1)
                local safety_gap = math.max(40, font_size * 2)
                local x = math.floor(width - (width + message.text_width + safety_gap) * progress)
                local lane = ((message.lane - 1) % lanes) + 1
                local y = top + (lane - 1) * lane_height
                local tags = string.format(
                    "{\\an7\\pos(%d,%d)\\fn%s\\b1\\fs%d\\bord%.1f\\shad1\\alpha&H%02X&\\c&H%s&}",
                    x, y, safe_font_name(config.font_family), font_size,
                    config.stroke_width, alpha, ass_color(message.color)
                )
                lines[#lines + 1] = tags .. to_ass_text(message.text)
                remaining[#remaining + 1] = message
            end
        end
    end
    for _, message in ipairs(active) do
        if message.positioned then
            -- Positioned danmaku are gated on the video clock, not on arrival
            -- time: IPC jitter and seek replay can deliver them early or late,
            -- and only the playback position decides when they should show.
            local vt = message.video_time
            local age = now - message.created
            local keep, render_it
            if vt == nil then
                keep = age < message.duration
                render_it = keep
            elseif pos < vt - 0.03 then
                -- Arrived early (Rust sends two seconds ahead); wait for the
                -- playback clock so the comment shows for its full window.
                keep = true
                render_it = false
            elseif pos > vt + message.duration then
                keep = false          -- display window already over
                render_it = false
            else
                keep = true           -- inside the display window
                render_it = true
            end
            if keep then
                remaining[#remaining + 1] = message
            end
            if render_it then
                local tags
                local font = clean_font_name(message.font) or config.font_family
                -- BAS font sizing: the official player renders on a 960x540
                -- stage and scales it onto the video. The font size lives in
                -- the p attribute (p[2], 25 = default), so a size-71 comment
                -- renders at 71 * height / 540 (142 px on 1080p). Earlier
                -- code read field 3 as the size, which is actually the
                -- duration seconds (2.35) and shrank every comment to ~5 px.
                local raw_size = tonumber(message.size) or 0.0
                local bas_size = raw_size > 0.0 and raw_size or 25.0
                local fs = math.max(12, math.floor(height * bas_size / 540.0))
                -- Motion progress follows the video clock so an early
                -- arrival does not jump straight to the end point.
                local progress = clamp((pos - (vt or 0)) / message.duration, 0, 1)
                -- BAS opacity can fade during the window ("0.25-0" fades out,
                -- "0.5-0.5" holds, "1-1" stays opaque). Interpolate between
                -- the start and end values; without a fade the semi-
                -- transparent copies of a lyric particle stack into a ghost
                -- image and blink off instead of dissolving.
                local opacity_now = message.alpha
                if message.alpha ~= nil and message.alpha_to ~= nil then
                    opacity_now = message.alpha
                        + (message.alpha_to - message.alpha) * progress
                end
                local msg_alpha = opacity_now ~= nil
                    and math.floor((1.0 - opacity_now) * 255 + 0.5) or alpha
                local bord = message.border == false and 0 or config.stroke_width
                local rot = (message.rotation and math.floor(message.rotation) ~= 0)
                    and string.format("\\frz%d", math.floor(message.rotation)) or ""
                -- Positioned (mode 7/8) danmaku normally carry normalized
                -- 0-1 coordinates in x/y. But Bilibili also ships "paired"
                -- comments (e.g. cherry-pop 去死: "去\u3000" + "\u3000死", same
                -- anchor, each with one full-width space that pushes its glyph
                -- to one side) whose text is a plain string, NOT a BAS
                -- [x,y,...] array. The Rust parser only resolves coordinates
                -- for the array form, so x/y arrive as nil here. The old code
                -- mapped nil -> 0, pinning both comments at the top-left
                -- corner where they overlapped. Instead, treat a missing
                -- coordinate as "centered" (0.5) so the shared-anchor +
                -- space trick works and the pair reads as one phrase.
                local has_coord = message.x ~= nil and message.y ~= nil
                local px_raw = tonumber(message.x) or 0.5
                local py_raw = tonumber(message.y) or 0.5
                if not has_coord then
                    px_raw = 0.5
                    py_raw = 0.5
                end
                local px2_raw = tonumber(message.x2) or px_raw
                local py2_raw = tonumber(message.y2) or py_raw
                -- Advanced danmaku coordinates are official-player percentages
                -- of the video area. A user-tunable scale (around the video
                -- center) plus pixel offsets lets playback match the official
                -- player when the BAS coordinate system drifts slightly.
                local scale = config.advanced_scale
                local off_x = config.advanced_offset_x
                local off_y = config.advanced_offset_y
                local cx, cy = width / 2.0, height / 2.0
                local map_x = function(v) return math.floor(v * width * scale + cx * (1 - scale) + off_x) end
                local map_y = function(v) return math.floor(v * height * scale + cy * (1 - scale) + off_y) end
                local px, py = map_x(px_raw), map_y(py_raw)
                local px2, py2 = map_x(px2_raw), map_y(py2_raw)
                -- Bilibili pairs two mode-7/8 comments that share one anchor
                -- and read as a single phrase (e.g. "去\u3000" + "\u3000死" =>
                -- 去死) by giving each a single full-width space that pushes
                -- its glyph to one side. The official player centers them on
                -- the shared point (an5); this renderer used an7 (top-left),
                -- so both glyphs were drawn from the same origin and
                -- overlapped. Only switch the anchor for space-padded text so
                -- every other advanced comment keeps its original alignment.
                -- Default advanced anchor is \an7 (top-LEFT of the text box at
                -- the point): positioned danmaku line up with the rest of the
                -- frame. Bilibili sends "去" and "死" as TWO separate mode-7
                -- comments with the SAME x/y; plain \an7 overlaps them on the
                -- point. Push "死" one glyph-width to the RIGHT of "去" (both
                -- top-aligned) so they read left-to-right as "去死" inside the
                -- shared box, with no vertical shift.
                local sp = "\227\128\128"
                local render_anchor, render_text, x_shift = "\an7", message.text, 0
                if string.sub(message.text, 1, 3) == sp then
                    render_anchor = "\an4"
                    render_text = string.sub(message.text, 4)
                    x_shift = math.floor(fs * 0.55)
                elseif string.sub(message.text, -3) == sp then
                    render_anchor = "\an6"
                    render_text = string.sub(message.text, 1, -4)
                    x_shift = -math.floor(fs * 0.55)
                elseif message.text == "去" then
                    render_anchor = "\an7"
                    render_text = "去"
                    x_shift = 0
                elseif message.text == "死" then
                    render_anchor = "\an7"
                    render_text = "死"
                    x_shift = math.floor(fs)
                end
                local pos_tag
                if message.x2 ~= nil and message.y2 ~= nil
                    and (message.x2 ~= message.x or message.y2 ~= message.y) then
                    local ix = math.floor(px + (px2 - px) * progress)
                    local iy = math.floor(py + (py2 - py) * progress)
                    pos_tag = string.format("\\pos(%d,%d)", ix + x_shift, iy)
                else
                    pos_tag = string.format("\\pos(%d,%d)", px + x_shift, py)
                end
                tags = string.format(
                    "{%s%s%s\\fn%s\\b1\\fs%d\\bord%.1f\\shad1\\alpha&H%02X&\\c&H%s&}",
                    render_anchor, pos_tag, rot, font, fs, bord, msg_alpha, ass_color(message.color)
                )
                lines[#lines + 1] = tags .. to_ass_text(render_text)
            end
        end
    end
    active = remaining
    -- Adjust the render cadence to the current load. Checking the active
    -- count and the timer rate is far cheaper than rebuilding the OSD at a
    -- fixed high rate; the actual rate change takes effect next frame.
    local fps = target_fps(#active)
    if fps ~= current_fps and refresh_timer ~= nil then
        current_fps = fps
        refresh_timer.timeout = 1 / fps
    end
    overlay.res_x, overlay.res_y = width, height
    overlay.data = table.concat(lines, "\n")
    overlay:update()
end

local function enqueue_message(message)
    if type(message) ~= "table" or type(message.text) ~= "string" then return end
    local text = message.text:gsub("[\r\n]", " ")
    if text == "" then return end
    local mode = tonumber(message.mode)
    -- The Rust side sends each danmaku at roughly its video timestamp, but
    -- IPC delays, seeks and send-retries can shift arrival by up to a second.
    -- Carry the true video time so rendering can gate on the playback clock
    -- instead of trusting the arrival moment.
    local video_time = tonumber(message.time)
    if mode == 7 or mode == 8 then
        -- Display duration comes from the BAS payload (field 9, ms). The
        -- official player shows exactly that window: a 320 ms comment lasts
        -- 320 ms. Earlier code clamped a 2 s floor, which made short lyric
        -- comments pile up on the same spot and overlap. Keep the raw value.
        local duration = math.max(0.1, (tonumber(message.duration) or 5000) / 1000)
        -- Deduplicate: after a backward seek the Rust side re-sends the
        -- batch, and without this check the same comment would be appended
        -- twice and render twice in the same spot (the classic overlap).
        -- The position is part of the identity: lyric "stacks" (same video
        -- time and text, slightly different x/y, e.g. 9 copies of 才不想要
        -- at y=0.808..0.818) are distinct renderings and must all show.
        -- Only a same-time, same-text, same-position duplicate is skipped.
        -- The position tolerance is deliberately tiny (0.00001 in normalized
        -- units, about 0.01 px on 1080p): particle-stack comments are spaced
        -- as little as 0.0005 apart, and a 0.001 threshold silently dropped
        -- whole stacks (the "disappearing danmaku" bug).
        local px = tonumber(message.x)
        local py = tonumber(message.y)
        -- Skip the rolling comments quickly: only positioned entries can be
        -- duplicates, and comparing every field against hundreds of scrollers
        -- per batch is pure waste in a dense section.
        for _, existing in ipairs(active) do
            if existing.positioned then
                if existing.video_time == video_time
                    and existing.text == text
                    and existing.x ~= nil and px ~= nil
                    and existing.y ~= nil and py ~= nil
                    and math.abs(existing.x - px) < 0.00001
                    and math.abs(existing.y - py) < 0.00001 then
                    return
                end
            end
        end
        active[#active + 1] = {
            text = text, color = message.color,
            positioned = true,
            x = tonumber(message.x),
            y = tonumber(message.y),
            x2 = tonumber(message.x2),
            y2 = tonumber(message.y2),
            rotation = tonumber(message.rotation),
            size = tonumber(message.size),
            font = message.font,
            alpha = tonumber(message.alpha),
            alpha_to = tonumber(message.alpha_to),
            border = message.border,
            video_time = video_time,
            own_duration = duration,
            created = now_clock(),
            duration = duration,
        }
        while #active > MAX_ACTIVE do table.remove(active, 1) end
        return
    end
    pending[#pending + 1] = {
        text = text, color = message.color, mode = mode or 1,
        video_time = video_time,
    }
    while #pending > MAX_PENDING do table.remove(pending, 1) end
end

local function on_danmaku(payload)
    if not config.enabled then return end
    enqueue_message(utils.parse_json(payload or ""))
end

local function on_danmaku_batch(payload)
    if not config.enabled then return end
    local messages = utils.parse_json(payload or "")
    if type(messages) ~= "table" then return end
    for _, message in ipairs(messages) do
        enqueue_message(message)
    end
end

local function on_config(payload)
    local value = utils.parse_json(payload or "")
    if type(value) ~= "table" then return end
    if type(value.enabled) == "boolean" then config.enabled = value.enabled end
    if type(value.massive_mode) == "boolean" then config.massive_mode = value.massive_mode end
    if type(value.font_family) == "string" then config.font_family = value.font_family end
    config.display_area = clamp(tonumber(value.display_area) or config.display_area, 0.1, 1.0)
    config.opacity = clamp(tonumber(value.opacity) or config.opacity, 0.0, 1.0)
    config.font_scale = clamp(tonumber(value.font_scale) or config.font_scale, 0.5, 2.5)
    config.duration = clamp(tonumber(value.duration) or config.duration, 3.0, 20.0)
    config.stroke_width = clamp(tonumber(value.stroke_width) or config.stroke_width, 0.0, 5.0)
    config.line_height = clamp(tonumber(value.line_height) or config.line_height, 1.0, 3.0)
    config.advanced_offset_x = tonumber(value.advanced_offset_x) or config.advanced_offset_x
    config.advanced_offset_y = tonumber(value.advanced_offset_y) or config.advanced_offset_y
    config.advanced_scale = clamp(tonumber(value.advanced_scale) or config.advanced_scale, 0.5, 2.0)
    lane_ready = {}
    if not config.enabled then active, pending = {}, {} end
    render()
end

mp.register_script_message("danmaku", on_danmaku)
mp.register_script_message("danmaku-batch", on_danmaku_batch)
mp.register_script_message("danmaku-config", on_config)
-- Start only after MPV reports the real display refresh rate, and recompute if
-- the window moves to another display. The video remains in audio-sync mode;
-- only the OSD cadence follows the display. The rate is capped at 60 fps: the
-- OSD rebuild is the expensive part, danmaku animation does not benefit from
-- panel-rate updates, and render() further drops the cadence under load. If
-- the compositor never reports a refresh rate (some Wayland setups), fall back
-- to 60 fps so the danmaku render loop always runs.
local function ensure_render_timer(fps)
    local rate = math.min(fps or 60, 60)
    if refresh_timer == nil then
        refresh_timer = mp.add_periodic_timer(1 / rate, render)
    else
        refresh_timer.timeout = 1 / rate
        refresh_timer:resume()
    end
end

local function apply_display_fps(value)
    local display_fps = tonumber(value)
    if display_fps == nil or display_fps <= 0 then
        ensure_render_timer(60)
        return false
    end
    ensure_render_timer(display_fps)
    if fps_probe_timer ~= nil then fps_probe_timer:stop() end
    return true
end

local function probe_display_fps()
    apply_display_fps(mp.get_property_number("display-fps", nil))
end

local function restart_fps_probe()
    if not apply_display_fps(mp.get_property_number("display-fps", nil))
        and fps_probe_timer ~= nil then
        fps_probe_timer:resume()
    end
end

fps_probe_timer = mp.add_periodic_timer(0.1, probe_display_fps)
mp.observe_property("display-fps", "native", function(_, value)
    if not apply_display_fps(value) then fps_probe_timer:resume() end
end)
mp.register_event("file-loaded", restart_fps_probe)
mp.register_event("video-reconfig", restart_fps_probe)
mp.add_timeout(0, restart_fps_probe)
mp.observe_property("pause", "bool", function(_, p)
    local is_paused = p == true
    if is_paused and not paused then
        -- Freeze: bank the time elapsed since the last play segment started.
        clock_offset = clock_offset + (mp.get_time() - clock_base)
        paused = true
    elseif not is_paused and paused then
        -- Resume: start a new play segment from "now"; the accumulator above
        -- already holds all prior unpaused time, so ages continue seamlessly.
        clock_base = mp.get_time()
        paused = false
    end
end)

mp.register_event("shutdown", function()
    if refresh_timer ~= nil then refresh_timer:kill() end
    if fps_probe_timer ~= nil then fps_probe_timer:kill() end
    overlay:remove()
end)
