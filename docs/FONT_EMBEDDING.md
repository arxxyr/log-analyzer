# 字体嵌入方案说明

## 概述

为了解决在没有中文字体的机器上运行时出现乱码的问题，我们将 Sarasa Term SC Nerd 字体直接嵌入到插件的 `.so` 文件中。

## 实现方式

### 1. 编译时嵌入

使用 Rust 的 `include_bytes!` 宏将字体文件编译进二进制：

```rust
const FONT_DATA: &[u8] = include_bytes!("../../../assests/fonts/SarasaTermSCNerd-Regular.ttf");
```

### 2. 运行时提取

首次运行时，插件会自动将嵌入的字体数据提取到系统字体目录：

- **Linux**: `~/.local/share/fonts/` 或 `~/.fonts/`
- **Windows**: `%LOCALAPPDATA%\Microsoft\Windows\Fonts\`
- **macOS**: `~/Library/Fonts/`
- **备选**: `/tmp/analyzer_fonts/`（临时目录）

### 3. 字体加载

使用 `plotters` 库的字体系统：
```rust
let font_loader = FontLoader::default();
let font_desc = font_loader.font_desc(14);  // 创建字体描述符
```

## 优势

1. **无需外部字体文件** - 字体数据打包在 `.so` 文件中，不会暴露原始 `.ttf` 文件
2. **自动安装** - 首次运行自动提取到系统字体目录，无需手动安装
3. **跨平台** - 自动适配 Linux/Windows/macOS 的字体目录结构
4. **性能优化** - 使用 `OnceLock` 确保字体只提取一次
5. **向后兼容** - 如果提取失败，会回退到系统默认字体

## 文件大小

- 原始字体文件: 29 MB
- 嵌入前的插件: 3.7 MB
- **嵌入后的插件: 32 MB** (29 MB 字体 + 3 MB 代码)

## 部署选项

### 部署模式

完全依赖嵌入字体，无需外部字体文件：
```bash
./scripts/deploy.sh
```

生成的 `bin/` 目录结构：
```
bin/                          (总计 67 MB)
├── analyzer                  (34 MB - 主程序)
├── plugins/
│   ├── libmaster_control_analyzer.so  (32 MB - 包含嵌入字体)
│   └── libcpp_demo_analyzer.so        (622 KB)
└── configs/
    └── analyzer.yaml         (4 KB)
```

**说明：**
- 字体已完全嵌入到 `libmaster_control_analyzer.so` 中
- 不再需要 `assests/fonts/` 目录
- 首次运行时会自动提取字体到系统目录

## 技术细节

### 字体提取逻辑

```rust
// 字体加载器会在创建时自动提取字体
let font_loader = FontLoader::new()?;

// 首次调用时，执行以下流程：
// 1. 检查用户字体目录是否可写
// 2. 将嵌入的字体数据写入文件
// 3. 验证文件完整性（大小匹配）
// 4. 缓存字体路径（使用 OnceLock）
```

### 防止重复提取

使用全局 `OnceLock` 确保线程安全且只提取一次：

```rust
static FONT_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

// 只在第一次调用时执行提取逻辑
FONT_PATH.get_or_init(|| {
    // 提取字体到系统目录
    // ...
})
```

### 字体验证

提取前会检查目标文件：
- 如果文件不存在 → 写入
- 如果文件大小不匹配 → 重新写入
- 如果文件已存在且大小正确 → 复用

## 相关文件

### 核心代码
- `plugins/master-control-analyzer/src/font_loader.rs` - 字体加载器实现
- `crates/analyzer-visualizer/src/font_loader.rs` - 可视化模块的字体加载器

### 修改的文件
- `plugins/master-control-analyzer/src/gantt.rs` - 甘特图生成（使用 FontLoader）
- `crates/analyzer-visualizer/src/lib.rs` - 可视化模块（使用 FontLoader）
- `plugins/master-control-analyzer/Cargo.toml` - 添加 `dirs` 依赖
- `crates/analyzer-visualizer/Cargo.toml` - 添加 `dirs` 依赖
- `scripts/deploy.sh` - 添加字体嵌入说明和可选复制逻辑

### 字体源文件（编译时）
- `assests/fonts/SarasaTermSCNerd-Regular.ttf` - 嵌入的字体文件（29 MB）

## 测试验证

### 检查字体是否嵌入
```bash
# 查看插件大小（应该是 32MB）
ls -lh bin/plugins/libmaster_control_analyzer.so

# 搜索字体名称（确认嵌入）
strings bin/plugins/libmaster_control_analyzer.so | grep "Sarasa"
```

### 运行时测试
```bash
# 清除之前提取的字体
rm -rf ~/.local/share/fonts/SarasaTermSCNerd-Regular.ttf

# 运行分析器（会自动提取字体）
cd bin
./analyzer auto

# 检查字体是否已提取
ls -lh ~/.local/share/fonts/SarasaTermSCNerd-Regular.ttf
```

## 注意事项

1. **首次运行延迟** - 首次运行时需要提取 29 MB 字体文件，可能需要 1-2 秒
2. **磁盘空间** - 字体会占用系统字体目录约 29 MB 空间
3. **权限要求** - 需要用户字体目录的写权限（通常无需管理员权限）
4. **字体缓存** - 某些系统可能需要刷新字体缓存才能使用新字体
   - Linux: `fc-cache -fv`
   - macOS: 重启应用或系统
   - Windows: 通常自动识别

## 安全性

- 字体数据在 `.so` 文件中，无法直接提取（需要逆向工程）
- 提取到用户字体目录后，文件权限为当前用户所有
- 不需要管理员权限，不会修改系统目录

## 未来改进

1. 支持多种字体样式（Bold、Italic 等）
2. 支持字体回退链（多个字体文件）
3. 支持字体压缩（减小 `.so` 文件体积）
4. 添加字体完整性校验（SHA256）
