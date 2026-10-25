using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace MdFileAssistant.Shell;

/// <summary>
/// 覆盖式更新：把 helper.bat 放进应用目录，让它等本进程退出后解压覆盖、再拉起新版。
///
/// 为什么必须假手一个脚本：Windows 锁着正在运行的 exe，进程内改自己改不掉。
/// ChinaSeal 的 0.4.20 修的就是"点了安装、软件退出、然后没有然后"——所以每一步都往
/// helper.log 里写，出问题时有一处可查，而不是静默失败。
///
/// v0.1.4 的零参数设计（取代旧的「命令行传 zip 与目录」）：
/// 1. 更新包统一复制到固定路径 `%TEMP%\MDReader-Update\update.zip`；
/// 2. 脚本本身放在应用目录，应用目录 = 脚本自己所在目录（`%~dp0`）；
/// 于是脚本**不需要任何命令行参数**——中文路径、空格、引号、OEM 码页这些
/// 旧的「参数传进来才安全」的顾虑从结构上消失，也不存在参数拼接。
///
/// 仍然保留的硬规矩：
/// - **bat 正文纯 ASCII**——cmd.exe 按 OEM 码页解码 .bat，任何非 ASCII 注释在中文系统上
///   会乱码、甚至被当成命令执行（v0.1.4 实测踩中：中文注释行在 GBK 机器上炸出
///   「不是内部或外部命令」）。零参数不豁免这条；
/// - 覆盖前先确认新版真的解出来了；每一步失败都保旧版、保 zip、拉回旧程序。
/// </summary>
public static class SelfReplace
{
    /// <summary>更新包的固定落脚路径（零参数契约的一部分）。</summary>
    public static string FixedZipPath => Path.Combine(Updater.StagingDir, "update.zip");

    public static string InstallAndRestart(string downloadedZipPath)
    {
        // 统一改名/复制到固定路径：即使下载器按资产名落盘，helper 也只认固定名
        var fixedZip = FixedZipPath;
        if (!string.Equals(Path.GetFullPath(downloadedZipPath), Path.GetFullPath(fixedZip), StringComparison.OrdinalIgnoreCase))
            File.Copy(downloadedZipPath, fixedZip, overwrite: true);

        var appDir = AppDir();
        var bat = Path.Combine(appDir, "MDReader-helper.bat");
        File.WriteAllText(bat, Body(), new UTF8Encoding(false));   // 正文全 ASCII，编码无关

        // 零参数启动：没有任何用户数据参与命令行
        NativeExec.RunHidden(bat);
        Updater.Log($"已交给 MDReader-helper.bat 覆盖重启：zip={fixedZip} appDir={appDir}");
        return bat;
    }

    /// <summary>exe 所在目录——更新就是把这个目录里的文件覆盖掉。</summary>
    public static string AppDir()
    {
        var exe = Environment.ProcessPath ?? typeof(SelfReplace).Assembly.Location;
        return Path.GetDirectoryName(Path.GetFullPath(exe))!;
    }

    private static string Body() => """
@echo off
setlocal
rem Zero-arg contract: update.zip is always at %TEMP%\MDReader-Update\update.zip,
rem and this script lives in the app dir, so APP_DIR is its own directory.
rem (%~dp0 ends with a backslash; the trailing dot normalizes it away.)
rem NOTE: keep this file pure ASCII - cmd.exe decodes .bat in the OEM codepage,
rem any non-ASCII comment here becomes garbage or even a bogus command on CJK systems.
set "ZIP=%TEMP%\MDReader-Update\update.zip"
set "APP_DIR=%~dp0."
set "STAGED=%TEMP%\MDReader-Update-Staged"
set "LOG=%TEMP%\MDReader-Update\helper.log"
echo === MDReader helper start %DATE% %TIME% === > "%LOG%"
echo ZIP=%ZIP% >> "%LOG%"
echo APP_DIR=%APP_DIR% >> "%LOG%"
if not exist "%ZIP%" (
  echo [!] ERROR: update.zip missing - nothing to install >> "%LOG%"
  exit /b 1
)
if exist "%STAGED%" rd /S /Q "%STAGED%" >> "%LOG%" 2>&1
mkdir "%STAGED%" >> "%LOG%" 2>&1
echo [1] extracting... >> "%LOG%"
powershell -NoProfile -ExecutionPolicy Bypass -Command "Expand-Archive -LiteralPath '%ZIP%' -DestinationPath '%STAGED%' -Force" >> "%LOG%" 2>&1
rem v0.1.4: stop immediately on extract failure (corrupt zip / disk full) -
rem the old script kept going and xcopy'd a half-empty staging dir over the install.
if errorlevel 1 goto :fail_wait
rem xcopy overwrites, so the old exe is always present after copy - the old
rem "check exe exists after copy" guard was always false and guarded nothing.
rem Check the staged output BEFORE copying instead.
if not exist "%STAGED%\MdFileAssistant.exe" goto :fail_wait
echo [2] waiting for the app to exit... >> "%LOG%"
powershell -NoProfile -Command "$ErrorActionPreference='SilentlyContinue'; while (Get-Process -Name 'MdFileAssistant') { Start-Sleep -Seconds 1 }; exit 0" >> "%LOG%" 2>&1
echo [2] wait exit=%ERRORLEVEL% >> "%LOG%"
echo [3] replacing files... >> "%LOG%"
xcopy /Y /E /I /Q "%STAGED%\*" "%APP_DIR%\" >> "%LOG%" 2>&1
rem v0.1.4: on xcopy failure (locked file / permission) the dir would be a mix of
rem old and new files - relaunch the old version so the user still has a working app.
if errorlevel 1 goto :fail_relaunch
echo [4] cleaning... >> "%LOG%"
rd /S /Q "%STAGED%" >> "%LOG%" 2>&1
rem v0.1.4: delete the zip only after a successful copy; keep it on failure
rem so the user can retry manually instead of losing the package.
del /F /Q "%ZIP%" >> "%LOG%" 2>&1
echo [5] relaunching... >> "%LOG%"
start "" "%APP_DIR%\MdFileAssistant.exe"
ping -n 2 127.0.0.1 >nul
echo [6] done >> "%LOG%"
del /F /Q "%~f0"
exit /b 0

:fail_wait
echo [!] ERROR: extract failed - waiting app exit, keeping old files and zip >> "%LOG%"
powershell -NoProfile -Command "$ErrorActionPreference='SilentlyContinue'; while (Get-Process -Name 'MdFileAssistant') { Start-Sleep -Seconds 1 }; exit 0" >> "%LOG%" 2>&1
start "" "%APP_DIR%\MdFileAssistant.exe"
exit /b 1

:fail_relaunch
echo [!] ERROR: xcopy failed - relaunching old version, keeping zip >> "%LOG%"
start "" "%APP_DIR%\MdFileAssistant.exe"
exit /b 1
""";
}
