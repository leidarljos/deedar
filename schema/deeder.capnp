# deeder Cap'n protocol. Writer and client speak these types.
# Schema id is the session identity for this file generation.
#
#   Client.create|get|list|trail|evidence|delete
#           |  this file
#           v
#        writer  -->  Deed + Evidence

@0xdeedd00dd00d0001;

enum Kind {
  file @0;
  set @1;
  quote @2;
  patch @3;
  mailDraft @4;
  clip @5;
  page @6;
  form @7;
  table @8;
  procedure @9;
  event @10;
}

enum Face {
  syntax @0;
  sheet @1;
  letter @2;
  waveform @3;
  document @4;
}

struct ProducedBy {
  agentId @0 :Text;
  activityId @1 :Text; # empty means none
}

struct Grant {
  union {
    host @0 :Text;
    path @1 :Text;
  }
}

struct Source {
  union {
    deed @0 :Text;
    path @1 :Text;
    url @2 :Text;
  }
}

struct Field {
  name @0 :Text;
  value @1 :Text;
}

struct Step {
  text @0 :Text;
  done @1 :Bool;
}

struct FileBody {
  path @0 :Text;
  mediaType @1 :Text;
}

struct SetBody {
  title @0 :Text;
  members @1 :List(Text);
}

struct QuoteBody {
  edition @0 :Text;
  start @1 :UInt64;
  end @2 :UInt64;
  excerpt @3 :Text;
  urls @4 :List(Text);
}

struct PatchBody {
  tree @0 :Text;
  diffs @1 :List(Text);
  functionaries @2 :List(Text);
}

struct MailDraftBody {
  messageId @0 :Text;
  inReplyTo @1 :Text;
  subject @2 :Text;
  path @3 :Text;
}

struct ClipBody {
  sources @0 :List(Text);
  inPoint @1 :Float64;
  outPoint @2 :Float64;
  duration @3 :Float64;
  path @4 :Text;
}

struct PageBody {
  url @0 :Text;
  snapshot @1 :Text;
}

struct FormBody {
  blank @0 :Text;
  fields @1 :List(Field);
  path @2 :Text;
}

struct TableBody {
  measures @0 :List(Field);
}

struct ProcedureBody {
  steps @0 :List(Step);
}

struct EventBody {
  when @0 :Text;
  where @1 :Text;
  who @2 :Text;
}

struct Body {
  union {
    file @0 :FileBody;
    set @1 :SetBody;
    quote @2 :QuoteBody;
    patch @3 :PatchBody;
    mailDraft @4 :MailDraftBody;
    clip @5 :ClipBody;
    page @6 :PageBody;
    form @7 :FormBody;
    table @8 :TableBody;
    procedure @9 :ProcedureBody;
    event @10 :EventBody;
  }
}

struct Deed {
  id @0 :Text;
  kind @1 :Kind;
  name @2 :Text;
  paths @3 :List(Text);
  sources @4 :List(Source);
  producedBy @5 :ProducedBy;
  grants @6 :List(Grant);
  face @7 :Face;
  body @8 :Body;
}

struct Evidence {
  deedId @0 :Text;
  producedBy @1 :ProducedBy;
  grants @2 :List(Grant);
  unixTime @3 :UInt64;
  signature @4 :Data;
}

struct CreateParams {
  id @0 :Text;
  name @1 :Text;
  sources @2 :List(Source);
  seatAgentId @3 :Text;
  seatActivityId @4 :Text;
  policyGrants @5 :List(Grant);
  body @6 :Body;
}

interface Deeder {
  create @0 (params :CreateParams) -> (deed :Deed, evidence :Evidence);
  get @1 (id :Text) -> (deed :Deed);
  list @2 () -> (deeds :List(Deed));
  trail @3 (id :Text) -> (deeds :List(Deed));
  delete @4 (id :Text) -> ();
  evidence @5 (id :Text) -> (evidence :Evidence);
}
