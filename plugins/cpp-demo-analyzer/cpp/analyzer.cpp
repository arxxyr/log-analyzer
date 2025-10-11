#include "analyzer.h"
#include "cpp-demo-analyzer/src/bridge.rs.h"  // 引入 cxx 生成的头文件
#include <fstream>
#include <iostream>
#include <regex>

namespace cpp_demo {

LogAnalyzer::LogAnalyzer() : round_count_(0) {
    std::cout << "[C++] LogAnalyzer 已创建" << std::endl;
}

LogAnalyzer::~LogAnalyzer() {
    std::cout << "[C++] LogAnalyzer 已销毁" << std::endl;
}

void LogAnalyzer::parse_log(const std::string& file) {
    std::ifstream ifs(file);
    if (!ifs) {
        throw std::runtime_error("无法打开文件: " + file);
    }
    
    std::string line;
    while (std::getline(ifs, line)) {
        log_lines_.push_back(line);
    }
    
    std::cout << "[C++] 解析了 " << log_lines_.size() << " 行日志" << std::endl;
}

void LogAnalyzer::detect_rounds() {
    std::regex round_start(R"(loop: 开始循环)");
    
    for (const auto& line : log_lines_) {
        if (std::regex_search(line, round_start)) {
            round_count_++;
        }
    }
    
    std::cout << "[C++] 检测到 " << round_count_ << " 个轮次" << std::endl;
}

void LogAnalyzer::generate_report(const std::string& output_dir) {
    std::string csv_file = output_dir + "/cpp_demo_analysis.csv";
    std::ofstream ofs(csv_file);
    
    if (ofs) {
        ofs << "round_id,line_count\n";
        for (int i = 0; i < round_count_; ++i) {
            ofs << i << "," << (log_lines_.size() / std::max(1, round_count_)) << "\n";
        }
        std::cout << "[C++] 生成报告: " << csv_file << std::endl;
    }
}

rust::Box<AnalysisResult> LogAnalyzer::analyze(
    rust::Str input_file,
    rust::Str output_dir
) {
    // 转换 rust::Str 到 std::string
    std::string input_path(input_file);
    std::string output_path(output_dir);

    std::cout << "[C++] 开始分析: " << input_path << std::endl;

    parse_log(input_path);
    detect_rounds();
    generate_report(output_path);

    // 构建输出文件列表
    rust::Vec<rust::String> output_files;
    output_files.push_back(rust::String(output_path + "/cpp_demo_analysis.csv"));

    // 使用 in_place 创建 AnalysisResult
    return rust::Box<AnalysisResult>::in_place(
        rust::String("C++ Demo 分析完成 - " +
                     std::to_string(round_count_) + " 轮次, " +
                     std::to_string(log_lines_.size()) + " 行"),
        std::move(output_files),
        static_cast<int32_t>(log_lines_.size()),
        static_cast<int32_t>(round_count_)
    );
}

std::unique_ptr<LogAnalyzer> new_log_analyzer() {
    return std::make_unique<LogAnalyzer>();
}

} // namespace cpp_demo
