// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: helpers of the background pipeline cache warm-up (vk_pipeline_serialization.cpp).

#pragma once

#include <algorithm>
#include <charconv>
#include <cstddef>
#include <deque>
#include <mutex>
#include <optional>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

namespace Vulkan {

inline bool UseOnDemandPipelines(bool binary_cache, bool supported, bool capture_only) {
    return binary_cache && supported && !capture_only;
}

/// Launcher "Shader pre-compilation" (GOW3_SHADER_PRECOMPILE): whether the saved pipelines are
/// built ahead of time, and whether the game waits for them.
enum class Precompile { Off, Background, Start };

inline Precompile ParsePrecompile(const char* value) {
    const std::string_view text = value ? value : "";
    return text == "background" ? Precompile::Background
           : text == "start"    ? Precompile::Start
                                : Precompile::Off;
}

/// Where a shader's GCN code lives in the game files, one line of the seed's locations.txt:
/// "0x<hash>", file relative to the game folder, offset of the code, length in bytes, tab separated.
struct SeedLocation {
    unsigned long long hash{};
    std::string file;
    unsigned long long start{};
    unsigned long long length{};
};

inline std::optional<SeedLocation> ParseSeedLocation(std::string_view line) {
    std::string_view parts[4];
    for (auto& part : parts) {
        const auto tab = line.find('\t');
        part = line.substr(0, tab);
        line = tab == std::string_view::npos ? std::string_view{} : line.substr(tab + 1);
    }
    SeedLocation loc;
    if (parts[0].starts_with("0x")) {
        parts[0].remove_prefix(2);
    }
    const auto number = [](std::string_view text, unsigned long long& out, int base) {
        const auto end = text.data() + text.size();
        const auto result = std::from_chars(text.data(), end, out, base);
        return !text.empty() && result.ec == std::errc{} && result.ptr == end;
    };
    if (parts[1].empty() || !number(parts[0], loc.hash, 16) || !number(parts[2], loc.start, 10) ||
        !number(parts[3], loc.length, 10) || !loc.hash || loc.length < 8) {
        return std::nullopt;
    }
    loc.file = parts[1];
    return loc;
}

/// Smallest permutation index (below 64, what the store keeps) that is not in `taken`.
inline std::optional<unsigned> FreePermutationIndex(const std::vector<unsigned>& taken) {
    for (unsigned idx = 0; idx < 64; ++idx) {
        if (std::ranges::find(taken, idx) == taken.end()) {
            return idx;
        }
    }
    return std::nullopt;
}

/// Warm-up threads: one on four cores or fewer, otherwise all but two, at most six.
inline unsigned WarmupThreadCount(unsigned cores) {
    return cores <= 4 ? 1u : std::min(cores - 2, 6u);
}

/// Items made on worker threads, handed to the GPU thread a few at a time so that a frame
/// never pays for the whole cache at once.
template <typename T>
class WarmupInbox {
public:
    void Push(T item) {
        std::scoped_lock lock{mutex};
        items.push_back(std::move(item));
    }

    /// At most `max` items, oldest first.
    std::vector<T> Take(std::size_t max) {
        std::scoped_lock lock{mutex};
        const std::size_t count = std::min(max, items.size());
        std::vector<T> out;
        out.reserve(count);
        for (std::size_t i = 0; i < count; ++i) {
            out.push_back(std::move(items.front()));
            items.pop_front();
        }
        return out;
    }

    bool Empty() {
        std::scoped_lock lock{mutex};
        return items.empty();
    }

private:
    std::mutex mutex;
    std::deque<T> items;
};

/// The first pipeline ready for a key is kept: a warm-up pipeline that arrives after the
/// game compiled the same one (or the reverse) is dropped. True when `value` was stored.
template <typename Map, typename Key, typename Value>
bool PublishFirst(Map& map, const Key& key, Value&& value) {
    return map.try_emplace(key, std::forward<Value>(value)).second;
}

} // namespace Vulkan
