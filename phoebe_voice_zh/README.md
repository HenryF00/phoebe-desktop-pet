# 《鸣潮》菲比中文语音

来源：[库街区《鸣潮》菲比词条](https://wiki.kurobbs.com/mc/item/1309523456688947200)

运行下载：

```bash
python3 download_phoebe_zh.py
```

输出内容：

- `wav/`：中文配音 WAV 文件
- `metadata.tsv`：文件名、语音标题、台词、来源链接和文件大小
- `metadata.json`：完整结构化元数据
- `phoebe_zh.list`：GPT-SoVITS 标注文件

脚本只提取词条中“中文”标签下的 WAV，并按来源 URL 去重。重复运行时会复用大小正确且具有 RIFF 文件头的已有文件。

仅建议用于个人、非商业研究。语音、角色及相关权利归各自权利人所有，请勿冒充配音演员或未经授权重新分发素材与模型。
