const concernPattern = /抱歉|遗憾|无法|不能|失败|错误|异常|风险|担心|注意安全|不确定|对不起|sorry|failed|error|unable|cannot/i;
const happyPattern = /你好|早上好|下午好|晚上好|晚安|谢谢|不客气|当然|太好了|恭喜|很高兴|祝你|没问题|可以呀|hello|thanks|congratulations/i;
const shyPattern = /喜欢你|爱你|好可爱|真可爱|很可爱|好漂亮|真漂亮|很漂亮|夸夸|害羞|心动|adorable|cute|love you|beautiful/i;
const nodPattern = /^(好的|好呀|可以|没问题|当然|是的|对|明白|收到|不客气|okay|ok|yes)[，。！!\s]/i;
const explainingPattern = /(^|\n)\s*(?:[-*]|\d+[.)、])\s+|首先|其次|最后|步骤|建议|结果|根据|具体来说|你可以|可以这样|以下|来源|文件|搜索/i;

export function inferReplyMotion({ replyText = "", userText = "", usedTool = false } = {}) {
  const reply = String(replyText).trim();
  const context = `${String(userText)}\n${reply}`;
  const emotion = concernPattern.test(reply)
    ? "concerned"
    : shyPattern.test(context)
      ? "shy"
      : happyPattern.test(reply)
        ? "happy"
        : "calm";

  let gesture = "idle";
  if (usedTool || explainingPattern.test(context)) gesture = "point";
  else if (nodPattern.test(reply) || (emotion === "happy" && reply.length < 180)) gesture = "nod";

  return { emotion, gesture };
}
