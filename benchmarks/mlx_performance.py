import time
import json
import requests
import argparse
import statistics

def benchmark(url, model, prompt, max_tokens=100, stream=True):
    print(f"Benchmarking {model} at {url}...")
    
    payload = {
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": max_tokens,
        "stream": stream,
        "temperature": 0.0
    }
    
    start_time = time.time()
    first_token_time = None
    token_count = 0
    
    try:
        response = requests.post(f"{url}/v1/chat/completions", json=payload, stream=stream)
        response.raise_for_status()
        
        if stream:
            for line in response.iter_lines():
                if line:
                    line = line.decode('utf-8')
                    if line.startswith("data: "):
                        data = line[6:]
                        if data == "[DONE]":
                            break
                        try:
                            chunk = json.loads(data)
                            if "choices" in chunk and len(chunk["choices"]) > 0:
                                delta = chunk["choices"][0]["delta"]
                                if "content" in delta:
                                    if first_token_time is None:
                                        first_token_time = time.time()
                                    token_count += 1
                        except:
                            pass
        else:
            data = response.json()
            first_token_time = time.time() # Approximate for non-streaming
            if "usage" in data:
                token_count = data["usage"]["completion_tokens"]
            else:
                # Estimate
                token_count = len(data["choices"][0]["message"]["content"].split()) * 1.3
                
    except Exception as e:
        print(f"Error: {e}")
        return None

    end_time = time.time()
    total_time = end_time - start_time
    
    ttft = (first_token_time - start_time) * 1000 if first_token_time else 0
    throughput = token_count / (end_time - first_token_time) if first_token_time and end_time > first_token_time else 0
    
    return {
        "ttft_ms": ttft,
        "throughput_tok_s": throughput,
        "total_tokens": token_count,
        "total_time_s": total_time
    }

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="MLX Performance Benchmark for Gumball")
    parser.add_argument("--url", default="http://127.0.0.1:8081", help="MLX server URL")
    parser.add_argument("--model", default="mlx-community/Qwen3-Next-80B-A3B-Instruct-4bit",
                        help="Model to benchmark (default: Qwen3-Next-80B-A3B for 64GB+ Mac)")
    parser.add_argument("--prompt", default="Write a short poem about rust programming language.",
                        help="Prompt to use for benchmarking")
    parser.add_argument("--iterations", type=int, default=5, help="Number of iterations")
    parser.add_argument("--max-tokens", type=int, default=100, help="Max tokens to generate")
    args = parser.parse_args()
    
    results = []
    for i in range(args.iterations):
        print(f"Iteration {i+1}/{args.iterations}")
        res = benchmark(args.url, args.model, args.prompt, max_tokens=args.max_tokens)
        if res:
            results.append(res)
            print(f"  TTFT: {res['ttft_ms']:.2f} ms")
            print(f"  Throughput: {res['throughput_tok_s']:.2f} tok/s")
        time.sleep(1)
        
    if results:
        avg_ttft = statistics.mean([r["ttft_ms"] for r in results])
        avg_throughput = statistics.mean([r["throughput_tok_s"] for r in results])
        
        print("\nResults Summary:")
        print(f"  Average TTFT: {avg_ttft:.2f} ms")
        print(f"  Average Throughput: {avg_throughput:.2f} tok/s")
        
        if avg_throughput >= 20:
            print("✅ KPI Passed: Throughput >= 20 tok/s")
        else:
            print("❌ KPI Failed: Throughput < 20 tok/s")
            
        if avg_ttft <= 250:
            print("✅ KPI Passed: TTFT <= 250 ms")
        else:
            print("❌ KPI Failed: TTFT > 250 ms")
