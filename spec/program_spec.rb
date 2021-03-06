require 'spec_helper'

describe "Program" do
  def parse(src)
    ast = Shiika::Parser.new.parse(src)
    return ast.to_program
  end

  it 'can be created' do
    prog = parse("class A; end; 1+1")
    sk_a = prog.sk_classes["A"]
    expect(sk_a.serialize).to eq({
      class: "SkClass",
      name: "A",
      superclass_template: Shiika::Type::TyRaw["Object"].to_s,
      sk_ivars: {},
      class_methods: {},
      sk_methods: {
        :initialize => {
          class: "SkInitializer",
          name: "initialize",
          params: [],
          ret_type_spec: Shiika::Type::TyRaw["Void"].to_s,
          body_stmts: [],
        }
      },
    })

    expect(prog.sk_main.serialize).to eq({
      class: "Main",
      stmts: [{
        class: "MethodCall",
        receiver_expr: {
          class: "Literal",
          value: 1
        },
        method_name: "+",
        args: [{
          class: "Literal",
          value: 1
        }]
      }]
    })
  end

  it 'can calculate type' do
    prog = parse("class A; end; 1")
    prog.add_type!
    expect(prog.sk_main.type).to eq(Shiika::Type::TyRaw["Int"])
  end
end
