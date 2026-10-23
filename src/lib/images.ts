// Images are scaled down before sending. Vision tokens grow with pixel
// count, so 1024 px on the long side keeps a picture near 1,000 tokens.
const MAX_SIDE = 1024;
const QUALITY = 0.85;
export const MAX_IMAGES = 4;

export async function toDataUri(file: Blob): Promise<string> {
  if (!file.type.startsWith("image/")) throw new Error("That file is not an image.");
  const bitmap = await createImageBitmap(file);
  const scale = Math.min(1, MAX_SIDE / Math.max(bitmap.width, bitmap.height));
  const w = Math.max(1, Math.round(bitmap.width * scale));
  const h = Math.max(1, Math.round(bitmap.height * scale));
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const g = canvas.getContext("2d");
  if (!g) throw new Error("Could not read the image.");
  g.fillStyle = "#ffffff"; // JPEG has no transparency
  g.fillRect(0, 0, w, h);
  g.drawImage(bitmap, 0, 0, w, h);
  bitmap.close();
  return canvas.toDataURL("image/jpeg", QUALITY);
}
