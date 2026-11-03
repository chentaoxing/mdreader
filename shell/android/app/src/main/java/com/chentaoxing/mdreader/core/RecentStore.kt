package com.chentaoxing.mdreader.core

import android.content.Context
import com.chentaoxing.mdreader.render.SymbolMode
import org.json.JSONArray
import org.json.JSONObject

/**
 * 最近打开 + 偏好记忆（SharedPreferences，最多 10 条，时间倒序，按 URI 去重）。
 *
 * 手机场景的刚需：SAF 每次都要翻目录，最近列表让「重读一份文档」变成两次点击。
 * 只存 SAF content URI 与展示名——SAF URI 不带 flag 是不能直接重开的，
 * 重开必须走 takePersistableUriPermission 持久化读权限（见 MainActivity）。
 */
object RecentStore {
    private const val PREFS = "mdreader"
    private const val KEY_RECENTS = "recents"
    private const val KEY_PRESET = "preset"
    private const val KEY_SYMBOL_MODE = "symbol_mode"
    private const val MAX = 10

    data class Entry(val uri: String, val name: String, val time: Long)

    private fun prefs(ctx: Context) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    fun recents(ctx: Context): List<Entry> {
        val arr = JSONArray(prefs(ctx).getString(KEY_RECENTS, "[]"))
        return (0 until arr.length()).map {
            val o = arr.getJSONObject(it)
            Entry(o.getString("uri"), o.getString("name"), o.getLong("time"))
        }
    }

    fun add(ctx: Context, uri: String, name: String) {
        val list = recents(ctx).filter { it.uri != uri }.toMutableList()
        list.add(0, Entry(uri, name, System.currentTimeMillis()))
        save(ctx, list.take(MAX))
    }

    fun remove(ctx: Context, uri: String) {
        save(ctx, recents(ctx).filter { it.uri != uri })
    }

    private fun save(ctx: Context, list: List<Entry>) {
        val arr = JSONArray()
        list.forEach { e ->
            arr.put(JSONObject().put("uri", e.uri).put("name", e.name).put("time", e.time))
        }
        prefs(ctx).edit().putString(KEY_RECENTS, arr.toString()).apply()
    }

    fun preset(ctx: Context): String =
        prefs(ctx).getString(KEY_PRESET, null) ?: "通用文章"

    fun setPreset(ctx: Context, p: String) {
        prefs(ctx).edit().putString(KEY_PRESET, p).apply()
    }

    fun symbolMode(ctx: Context): SymbolMode {
        val s = prefs(ctx).getString(KEY_SYMBOL_MODE, null) ?: return SymbolMode.Dim
        return runCatching { SymbolMode.valueOf(s) }.getOrDefault(SymbolMode.Dim)
    }

    fun setSymbolMode(ctx: Context, mode: SymbolMode) {
        prefs(ctx).edit().putString(KEY_SYMBOL_MODE, mode.name).apply()
    }
}
