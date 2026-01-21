#pragma once
#include <string>
#include <vector>
#include <memory>
#include "rust/cxx.h"  // cxx 类型

namespace cpp_demo {

// 前向声明 cxx 生成的结构体
struct AnalysisResult;

/// C++ 日志分析器
class LogAnalyzer {
public:
    LogAnalyzer();
    ~LogAnalyzer();

    // 主分析函数（返回 cxx 生成的 Box<AnalysisResult>）
    rust::Box<AnalysisResult> analyze(
        rust::Str input_file,
        rust::Str output_dir
    );
    
private:
    void parse_log(const std::string& file);
    void detect_rounds();
    void generate_report(const std::string& output_dir);
    
    std::vector<std::string> log_lines_;
    int round_count_;
};

// 工厂函数（供 cxx 调用）
std::unique_ptr<LogAnalyzer> new_log_analyzer();

} // namespace cpp_demo
