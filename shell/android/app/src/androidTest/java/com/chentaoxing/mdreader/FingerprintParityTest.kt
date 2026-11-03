package com.chentaoxing.mdreader

import androidx.test.platform.app.InstrumentationRegistry
import com.chentaoxing.mdreader.core.Envelope
import com.chentaoxing.mdreader.core.MdCore
import com.chentaoxing.mdreader.core.Wire
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 五平台一致性门的 Android 侧（规格第四十章）：
 * 33 份语料在真机/模拟器上跑出的「输入正文校验和」必须与 Windows/Linux 基线逐字节一致。
 *
 * 期望值来自仓库基线 fingerprint-linux-baseline.txt（本机 Windows 亦逐字节复验过）。
 */
class FingerprintParityTest {

    private fun assets(name: String): ByteArray =
        InstrumentationRegistry.getInstrumentation().context.assets.open(name).use { it.readBytes() }

    @Test
    fun corpusChecksumsMatchDesktopBaseline() {
        val expected: JSONObject = JSONObject(
            assets("expected_fingerprints.json").decodeToString()
        )
        var checked = 0
        val failures = mutableListOf<String>()

        for (name in expected.keys()) {
            val bytes = assets("corpus/$name")
            val env = Envelope.parse(MdCore.optimize(bytes, "通用文章", "preserve"))
            if (env !is Envelope.Ok) {
                failures.add("$name: 状态非 0")
                continue
            }
            val r = Wire.optimize(env.payload)
            val exp = expected.getJSONObject(name)
            // checksum_original_str 与桌面基线的输入正文指纹（冒号前 16 进制）对应
            // u64 溢出两道坑：parseUnsignedLong 的结果再用 toString() 打印仍是带符号；
            // 必须 toUnsignedString 才是桌面 FFI「u64 → 十进制字符串」的同款口径
            val expChecksum = java.lang.Long.toUnsignedString(java.lang.Long.parseUnsignedLong(exp.getString("in"), 16))
            if (r.checksumOriginalStr != expChecksum) {
                failures.add("$name: 输入校验和 ${r.checksumOriginalStr} ≠ 基线 $expChecksum")
            }
            if (r.contentChanges != 0) failures.add("$name: 正文变化 ${r.contentChanges} ≠ 0")
            checked++
        }

        assertEquals("33 份语料全部要被检查", expected.length(), checked)
        assertTrue("与桌面基线不一致：\n${failures.joinToString("\n")}", failures.isEmpty())
    }

    @Test
    fun engineVersionMatchesRelease() {
        assertEquals("0.1.4", MdCore.engineVersion)
    }
}
