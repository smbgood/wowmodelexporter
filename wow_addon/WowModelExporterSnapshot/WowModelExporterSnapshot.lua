local ADDON_NAME = "WowModelExporterSnapshot"
local SLOT_COUNT = 19

local SLOT_NAMES = {
  [1] = "head",
  [2] = "neck",
  [3] = "shoulder",
  [4] = "shirt",
  [5] = "chest",
  [6] = "waist",
  [7] = "legs",
  [8] = "feet",
  [9] = "wrist",
  [10] = "hands",
  [11] = "finger1",
  [12] = "finger2",
  [13] = "trinket1",
  [14] = "trinket2",
  [15] = "back",
  [16] = "mainhand",
  [17] = "offhand",
  [18] = "ranged",
  [19] = "tabard",
}

local function getSexLabel()
  if UnitSex then
    local sex = UnitSex("player")
    if sex == 2 then
      return "male"
    end
    if sex == 3 then
      return "female"
    end
  end
  return "unknown"
end

local function parseItemId(link)
  if not link then
    return nil
  end
  local itemId = string.match(link, "item:(%d+)")
  if itemId then
    return tonumber(itemId)
  end
  return nil
end

local function collectGearSnapshot()
  local gear = {}
  for slot = 1, SLOT_COUNT do
    local slotName = SLOT_NAMES[slot] or tostring(slot)
    local link = GetInventoryItemLink("player", slot)
    local texture = GetInventoryItemTexture("player", slot)
    gear[slotName] = {
      slot = slot,
      itemLink = link,
      itemId = parseItemId(link),
      texture = texture,
    }
  end
  return gear
end

local function compactPayload(snapshot)
  local chunks = {
    "realm=" .. (snapshot.realm or "unknown"),
    "name=" .. (snapshot.characterName or "unknown"),
    "race=" .. (snapshot.race or "unknown"),
    "sex=" .. (snapshot.sex or "unknown"),
    "class=" .. (snapshot.class or "unknown"),
  }

  if snapshot.gear then
    for slotName, entry in pairs(snapshot.gear) do
      if entry.itemId then
        table.insert(chunks, slotName .. "=" .. tostring(entry.itemId))
      end
    end
  end

  table.sort(chunks)
  return "WME_SNAPSHOT|" .. table.concat(chunks, "|")
end

local function storeSnapshot()
  if not WME_SnapshotsDB then
    WME_SnapshotsDB = {}
  end
  if not WME_SnapshotsDB.snapshots then
    WME_SnapshotsDB.snapshots = {}
  end
  if not WME_SnapshotsDB.compactSnapshots then
    WME_SnapshotsDB.compactSnapshots = {}
  end

  local _, race = UnitRace("player")
  local _, class = UnitClass("player")
  local snapshot = {
    id = #WME_SnapshotsDB.snapshots + 1,
    createdAt = date("%Y-%m-%d %H:%M:%S"),
    createdAtEpoch = time(),
    realm = GetRealmName(),
    characterName = UnitName("player"),
    race = race,
    sex = getSexLabel(),
    class = class,
    level = UnitLevel("player"),
    gear = collectGearSnapshot(),
  }

  table.insert(WME_SnapshotsDB.snapshots, snapshot)
  WME_SnapshotsDB.lastPayload = compactPayload(snapshot)
  snapshot.payload = WME_SnapshotsDB.lastPayload
  table.insert(WME_SnapshotsDB.compactSnapshots, WME_SnapshotsDB.lastPayload)

  DEFAULT_CHAT_FRAME:AddMessage("|cff33ff99WME|r Snapshot saved #" .. snapshot.id)
  DEFAULT_CHAT_FRAME:AddMessage("|cff33ff99WME|r " .. WME_SnapshotsDB.lastPayload)
  DEFAULT_CHAT_FRAME:AddMessage("|cff33ff99WME|r Snapshots persist on /reload or logout.")
end

SLASH_WMESNAPSHOT1 = "/wmesnap"
SLASH_WMESNAPSHOT2 = "/wmeexport"
SlashCmdList["WMESNAPSHOT"] = function()
  storeSnapshot()
end

local frame = CreateFrame("Button", "WME_SnapshotButton", UIParent, "UIPanelButtonTemplate")
frame:SetWidth(130)
frame:SetHeight(24)
frame:SetPoint("TOP", UIParent, "TOP", 0, -120)
frame:SetText("Save Gear Snapshot")
frame:RegisterForClicks("LeftButtonUp")
frame:SetScript("OnClick", function()
  storeSnapshot()
end)
