// Public facts about a model this harness is tuned for (from its Hugging Face
// card). Shown in About when the active profile points at it. Everything about
// the user's own server comes live from the profile and the server instead.

export const MODEL_CARD = {
  name: "Qwen3.6-35B-A3B-Uncensored-HauhauCS-Aggressive",
  file: "…-Q4_K_M.gguf (21.2 GB) plus the mmproj f16 projector for vision",
  base: "Qwen/Qwen3.6-35B-A3B",
  quant: "Q4_K_M, 21.2 GB",
  architecture:
    "Mixture of Experts (qwen35moe): 35B total, about 3B active per token, 256 experts (8 routed), 40 layers, hybrid linear and full attention (3:1)",
  context: "262K native; serve what fits your memory (64K on a 10 GB GPU with expert offload)",
  vision: "mmproj f16 projector, 899 MB: images in, text out",
  alignment: "Uncensored \"Aggressive\" variant: doesn't refuse; may add short disclaimers",
  thinking: "Thinks by default; the app turns it off unless Think is on",
  weakness: "Not reliable at exact counting or recall",
} as const;

/** True when the active profile looks like it serves the model above. */
export function isKnownModel(model: string | undefined, modelInfo: string | undefined): boolean {
  const s = `${model ?? ""} ${modelInfo ?? ""}`.toLowerCase();
  return s.includes("qwen3.6-35b-a3b") || s.includes("qwen35");
}
