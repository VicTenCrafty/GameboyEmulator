import os

def count_lines(directory):
    total_lines = 0
    for root, _, files in os.walk(directory):
        for file in files:
            if file.endswith(".rs"):
                path = os.path.join(root, file)
                with open(path, "r", encoding="utf-8") as f:
                    total_lines += sum(1 for _ in f)
    return total_lines

if __name__ == "__main__":
    project_dir = os.getcwd()
    total = count_lines(project_dir)
    print(f"Total lines of Rust code: {total}")
