"""Fixed global-search and local-engine benchmark budgets."""

GLOBAL = (
    {"depth": 1, "milliseconds": 300, "analysis_nodes": 32},
    {"depth": 2, "milliseconds": 600, "analysis_nodes": 64},
    {"depth": 3, "milliseconds": 1000, "analysis_nodes": 128},
    {"depth": 4, "milliseconds": 1600, "analysis_nodes": 128},
    {"depth": 5, "milliseconds": 2400, "analysis_nodes": 256},
    {"depth": 6, "milliseconds": 4000, "analysis_nodes": 256},
)
LOCAL = (
    {"skill": 0, "milliseconds": 60},
    {"skill": 2, "milliseconds": 100},
    {"skill": 5, "milliseconds": 180},
    {"skill": 10, "milliseconds": 300},
    {"skill": 15, "milliseconds": 500},
    {"skill": 20, "milliseconds": 900},
)
