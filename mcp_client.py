#!/usr/bin/env python3
import json
import sys
import os
import time
import subprocess
import argparse
from typing import Any, Dict, List, Optional, Union
from dataclasses import dataclass
from enum import Enum

class MCPError(Exception):
    """Base exception for MCP client errors"""
    pass

class ToolError(MCPError):
    """Raised when a tool operation fails"""
    pass

@dataclass
class JsonRpcRequest:
    method: str
    params: Dict[str, Any]
    id: int = 1
    jsonrpc: str = "2.0"

    def to_dict(self) -> Dict[str, Any]:
        return {
            "jsonrpc": self.jsonrpc,
            "method": self.method,
            "params": self.params,
            "id": self.id
        }

class MCPClient:
    def __init__(self, host: str = "localhost", port: int = 3000):
        self._request_id = 0
        self._server_process = None
        self._ensure_directories()

    def _ensure_directories(self):
        """Ensure required directories exist"""
        home = os.path.expanduser("~")
        dirs = [
            os.path.join(home, "Developer", ".mcp"),
            os.path.join(home, "Developer", ".mcp", "logs"),
            os.path.join(home, "Developer", ".mcp", "knowledge_graph"),
            os.path.join(home, "Developer", ".mcp", "thoughts")
        ]
        for d in dirs:
            os.makedirs(d, exist_ok=True)

    def _ensure_server_running(self):
        if not self._server_process or self._server_process.poll() is not None:
            script_dir = os.path.dirname(os.path.abspath(__file__))
            server_script = os.path.join(script_dir, "mcp-server.sh")
            
            # Set up environment variables
            env = os.environ.copy()
            env["RUST_LOG"] = "web_scrape_mcp=debug,info"
            env["RUST_BACKTRACE"] = "1"
            env["LOG_DIR"] = os.path.expanduser("~/Developer/.mcp/logs")
            env["KNOWLEDGE_GRAPH_DIR"] = os.path.expanduser("~/Developer/.mcp/knowledge_graph")
            env["THOUGHTS_DIR"] = os.path.expanduser("~/Developer/.mcp/thoughts")
            
            self._server_process = subprocess.Popen(
                [server_script],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                bufsize=1,  # Line buffered
                env=env
            )
            
            # Wait a bit for server to start
            time.sleep(1)
            
            if self._server_process.poll() is not None:
                stderr = self._server_process.stderr.read()
                raise MCPError(f"Server failed to start: {stderr}")

    def _get_next_id(self) -> int:
        self._request_id += 1
        return self._request_id

    def _make_request(self, method: str, params: Dict[str, Any]) -> Dict[str, Any]:
        self._ensure_server_running()
        
        request = JsonRpcRequest(
            method=method,
            params=params,
            id=self._get_next_id()
        )
        
        try:
            # Send request to server process
            request_json = json.dumps(request.to_dict()) + "\n"
            self._server_process.stdin.write(request_json)
            self._server_process.stdin.flush()
            
            # Read response from server
            response = self._server_process.stdout.readline()
            if not response:
                stderr = self._server_process.stderr.read()
                raise MCPError(f"No response received from server. stderr: {stderr}")
                
            result = json.loads(response)
            
            if "error" in result:
                raise ToolError(f"Tool error: {result['error']}")
                
            return result.get("result", {})
        except json.JSONDecodeError as e:
            stderr = self._server_process.stderr.read()
            raise MCPError(f"Failed to parse server response: {e}. stderr: {stderr}")

    def sequential_thinking(self, action: str, **params) -> Dict[str, Any]:
        """
        Interact with the sequential thinking tool
        
        Actions:
        - add: Add a new thought
        - revise: Revise an existing thought
        - branch: Create a branch from existing thought
        """
        tool_params = {}
        if action == "add":
            tool_params = {
                "content": params.get("content"),
                "total_thoughts": int(params.get("total", 1))
            }
            action = "add_thought"
        elif action == "revise":
            tool_params = {
                "content": params.get("content"),
                "revises_number": int(params.get("revises"))
            }
            action = "revise_thought"
        elif action == "branch":
            tool_params = {
                "content": params.get("content"),
                "branch_from": int(params.get("branch_from")),
                "branch_id": params.get("branch_id")
            }
            action = "branch_thought"
            
        return self._make_request("sequential_thinking", {
            "action": action,
            "params": tool_params
        })

    def memory(self, action: str, **params) -> Dict[str, Any]:
        """
        Interact with the memory tool
        
        Actions:
        - memorize_thought: Store a thought in memory
        - connect_thoughts: Create connections between thoughts
        - search_memory: Search through memorized thoughts
        """
        return self._make_request("memory", {
            "action": action,
            "params": params
        })

    def graph_tool(self, action: str, **params) -> Dict[str, Any]:
        """
        Interact with the graph tool
        
        Actions:
        - create_root: Create a root node
        - create_node: Create a new node
        - get_node: Retrieve a node
        - search_nodes: Search through nodes
        """
        return self._make_request("graph_tool", {
            "action": action,
            "params": params
        })

    def task_planning(self, action: str, **params) -> Dict[str, Any]:
        """
        Interact with the task planning tool
        
        Actions:
        - create_task: Create a new task
        - update_task: Update task status
        - get_task: Get task details
        """
        return self._make_request("task_planning", {
            "action": action,
            "params": params
        })

    def brave_search(self, query: str) -> Dict[str, Any]:
        """Perform a search using Brave Search"""
        return self._make_request("brave_search", {"query": query})

    def scrape_url(self, url: str, **params) -> Dict[str, Any]:
        """Scrape content from a URL"""
        return self._make_request("scrape_url", {"url": url, **params})

    def git(self, action: str, **params) -> Dict[str, Any]:
        """
        Interact with git tool
        
        Actions:
        - init_repo: Initialize a repository
        - add_files: Stage files
        - commit_changes: Commit staged changes
        """
        return self._make_request("git", {
            "action": action,
            "params": params
        })

def create_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="MCP Client - Command line interface for MCP tools")
    parser.add_argument("--host", default="localhost", help="MCP server host")
    parser.add_argument("--port", type=int, default=3000, help="MCP server port")
    
    subparsers = parser.add_subparsers(dest="tool", help="MCP tool to use")
    
    # Sequential Thinking
    think_parser = subparsers.add_parser("think", help="Sequential thinking tool")
    think_subparsers = think_parser.add_subparsers(dest="action")

    # Add thought
    add_parser = think_subparsers.add_parser("add")
    add_parser.add_argument("--content", required=True, help="Content of the thought")
    add_parser.add_argument("--total", type=int, default=1, help="Total number of thoughts")

    # Revise thought
    revise_parser = think_subparsers.add_parser("revise")
    revise_parser.add_argument("--content", required=True, help="Content of the thought")
    revise_parser.add_argument("--revises", type=int, required=True, help="Number of thought to revise")

    # Branch thought
    branch_parser = think_subparsers.add_parser("branch")
    branch_parser.add_argument("--content", required=True, help="Content of the thought")
    branch_parser.add_argument("--branch-from", type=int, required=True, help="Thought number to branch from")
    branch_parser.add_argument("--branch-id", required=True, help="Branch identifier")

    # Memory
    mem_parser = subparsers.add_parser("memory", help="Memory tool")
    mem_parser.add_argument("action", choices=["memorize", "connect", "search"], help="Action to perform")
    mem_parser.add_argument("--thought", type=int, help="Thought number to memorize")
    mem_parser.add_argument("--tags", nargs="+", help="Tags for the thought")
    mem_parser.add_argument("--from-thought", type=int, help="Source thought for connection")
    mem_parser.add_argument("--to-thought", type=int, help="Target thought for connection")
    mem_parser.add_argument("--relation", help="Relation type for connection")
    mem_parser.add_argument("--query", help="Search query")

    # Graph Tool
    graph_parser = subparsers.add_parser("graph", help="Graph tool")
    graph_parser.add_argument("action", choices=["create-root", "create-node", "get", "search"], help="Action to perform")
    graph_parser.add_argument("--name", help="Node name")
    graph_parser.add_argument("--description", help="Node description")
    graph_parser.add_argument("--content", help="Node content")
    graph_parser.add_argument("--parent", help="Parent node name")
    graph_parser.add_argument("--relation", help="Relation to parent")

    # Task Planning
    task_parser = subparsers.add_parser("task", help="Task planning tool")
    task_parser.add_argument("action", choices=["create", "update", "get"], help="Action to perform")
    task_parser.add_argument("--title", help="Task title")
    task_parser.add_argument("--description", help="Task description")
    task_parser.add_argument("--priority", type=int, help="Task priority")
    task_parser.add_argument("--status", help="Task status")
    task_parser.add_argument("--task-id", help="Task ID")

    # Brave Search
    search_parser = subparsers.add_parser("search", help="Brave search tool")
    search_parser.add_argument("query", help="Search query")

    # URL Scraping
    scrape_parser = subparsers.add_parser("scrape", help="URL scraping tool")
    scrape_parser.add_argument("url", help="URL to scrape")

    # Git
    git_parser = subparsers.add_parser("git", help="Git tool")
    git_parser.add_argument("action", choices=["init", "add", "commit"], help="Action to perform")
    git_parser.add_argument("--files", nargs="+", help="Files to add")
    git_parser.add_argument("--message", help="Commit message")

    return parser

def main():
    parser = argparse.ArgumentParser(description='MCP Client')
    subparsers = parser.add_subparsers(dest='command')

    # Sequential thinking
    think_parser = subparsers.add_parser('think')
    think_subparsers = think_parser.add_subparsers(dest='action')

    # Add thought
    add_parser = think_subparsers.add_parser('add')
    add_parser.add_argument('--content', required=True, help='Content of the thought')
    add_parser.add_argument('--total', type=int, default=1, help='Total number of thoughts')

    # Revise thought
    revise_parser = think_subparsers.add_parser('revise')
    revise_parser.add_argument('--content', required=True, help='Content of the thought')
    revise_parser.add_argument('--revises', type=int, required=True, help='Number of thought to revise')

    # Branch thought
    branch_parser = think_subparsers.add_parser('branch')
    branch_parser.add_argument('--content', required=True, help='Content of the thought')
    branch_parser.add_argument('--branch-from', type=int, required=True, help='Thought number to branch from')
    branch_parser.add_argument('--branch-id', required=True, help='Branch identifier')

    args = parser.parse_args()
    client = MCPClient()

    if args.command == 'think':
        if args.action == 'add':
            result = client.sequential_thinking('add', content=args.content, total=args.total)
        elif args.action == 'revise':
            result = client.sequential_thinking('revise', content=args.content, revises=args.revises)
        elif args.action == 'branch':
            result = client.sequential_thinking('branch', content=args.content, branch_from=args.branch_from, branch_id=args.branch_id)
        else:
            print("Unknown action")
            return
        print(json.dumps(result, indent=2))

if __name__ == "__main__":
    main()
