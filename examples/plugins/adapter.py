"""Replace chat() with a real service, SDK, or local model call."""
from olyx_plugin import serve


def chat(model, request):
    # Return an OpenAI-compatible completion object, for example:
    # {"choices": [{"message": {"role": "assistant", "content": text},
    #               "finish_reason": "stop"}]}
    # Read any credentials from your environment, never from this file.
    raise NotImplementedError("Connect your model here before routing requests.")


if __name__ == "__main__":
    serve(chat)
