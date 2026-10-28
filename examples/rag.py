"""Application-owned RAG example using Olyx HTTP endpoints and Python's stdlib.

Index UTF-8 .txt/.md files, retrieve chunks locally, and ask a configured model.
This example owns its JSON vector index; Olyx only provides model access/traces.
"""
import argparse
import json
import math
import os
from pathlib import Path
from urllib.request import Request, urlopen


def post(base_url, path, body):
    headers = {'Content-Type': 'application/json'}
    if os.environ.get('OLYX_SERVER_API_KEY'):
        headers['Authorization'] = 'Bearer ' + os.environ['OLYX_SERVER_API_KEY']
    request = Request(base_url.rstrip('/') + path, data=json.dumps(body).encode(), headers=headers)
    with urlopen(request, timeout=120) as response:
        return json.load(response)


def cosine(left, right):
    if len(left) != len(right):
        raise ValueError('Vector dimensions differ; rebuild the index with the query embedding model')
    denominator = math.sqrt(sum(x*x for x in left) * sum(x*x for x in right))
    return sum(a*b for a, b in zip(left, right)) / denominator if denominator else 0.0


def index_documents(args):
    root = Path(args.documents)
    if not root.is_dir():
        raise ValueError('documents must be a directory of UTF-8 .txt or .md files')
    chunks = []
    for path in sorted(root.rglob('*')):
        if path.is_file() and path.suffix.lower() in {'.txt', '.md'}:
            text = path.read_text(encoding='utf-8')
            for offset in range(0, len(text), 1600):
                chunk = text[offset:offset+1800]
                if chunk.strip(): chunks.append({'source': str(path.relative_to(root)), 'text': chunk})
    if not chunks:
        raise ValueError('No nonempty UTF-8 .txt or .md documents found')
    vectors = []
    model, dimensions = None, None
    for offset in range(0, len(chunks), 32):
        result = post(args.base_url, '/embeddings', {'model': args.embedding_model, 'input': [c['text'] for c in chunks[offset:offset+32]], 'encoding_format': 'float'})
        batch = sorted(result['data'], key=lambda item: item['index'])
        if len(batch) != len(chunks[offset:offset+32]): raise ValueError('Embedding count differs from chunk count')
        if model is not None and result['model'] != model: raise ValueError('Embedding model changed during indexing')
        model = result['model']
        for item in batch:
            vector = item['embedding']
            if dimensions is not None and len(vector) != dimensions: raise ValueError('Embedding dimensions changed during indexing')
            dimensions = len(vector)
            vectors.append(vector)
    value = {'embedding_alias': args.embedding_model, 'embedding_model': model, 'dimensions': dimensions,
             'chunks': [{**chunk, 'embedding': vector} for chunk, vector in zip(chunks, vectors)]}
    Path(args.index).write_text(json.dumps(value), encoding='utf-8')
    print(f'Indexed {len(chunks)} chunks in {args.index}')


def ask(args):
    index = json.loads(Path(args.index).read_text(encoding='utf-8'))
    result = post(args.base_url, '/embeddings', {'model': index['embedding_alias'], 'input': args.question, 'encoding_format': 'float'})
    vector = result['data'][0]['embedding']
    if result['model'] != index['embedding_model'] or len(vector) != index['dimensions']:
        raise ValueError('Query embedding model or dimensions differ; rebuild the index')
    ranked = sorted(index['chunks'], key=lambda chunk: cosine(vector, chunk['embedding']), reverse=True)[:3]
    context = '\n\n'.join(f'[{i+1}] {chunk["source"]}\n{chunk["text"]}' for i, chunk in enumerate(ranked))
    answer = post(args.base_url, '/chat/completions', {'model': args.answer_model, 'messages': [
        {'role': 'system', 'content': 'Answer using the supplied source excerpts. Cite sources as [1], [2], or [3]. If they do not contain the answer, say so.'},
        {'role': 'user', 'content': f'Source excerpts:\n{context}\n\nQuestion: {args.question}'}]})
    print(answer['choices'][0]['message']['content'])
    for i, chunk in enumerate(ranked): print(f'[{i+1}] {chunk["source"]}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    build = commands.add_parser('index')
    build.add_argument('documents')
    build.add_argument('--embedding-model', default='embed')
    query = commands.add_parser('ask')
    query.add_argument('question')
    query.add_argument('--answer-model', default='answer')
    for command in (build, query):
        command.add_argument('--index', default='rag-index.json')
        command.add_argument('--base-url', default='http://127.0.0.1:8787/v1')
    args = parser.parse_args()
    try:
        index_documents(args) if args.command == 'index' else ask(args)
    except (ValueError, OSError, KeyError) as error:
        parser.exit(1, f'{error}\n')


if __name__ == '__main__': main()
